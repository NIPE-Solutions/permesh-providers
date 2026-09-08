// SPDX-License-Identifier: MIT
//! Fixed-endpoint refresh-token exchange. No token cache or persistence.
use permesh_provider_sdk::ProviderError;
use permesh_secrets::Secret;
use reqwest::{Client, Url};
use serde::Deserialize;
use std::time::Duration;
use zeroize::Zeroizing;
const MAX_BODY: usize = 64 * 1024;
const MAX_TOKEN: usize = 16 * 1024;
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

pub(super) async fn refresh(
    client_id: &str,
    refresh_token: &Secret,
    client_secret: &Secret,
) -> Result<Secret, ProviderError> {
    let endpoint = Url::parse(TOKEN_ENDPOINT).map_err(|_| crate::error("configuration"))?;
    exchange(client_id, refresh_token, client_secret, endpoint).await
}
fn auth_error() -> ProviderError {
    ProviderError::new(
        "unauthorized",
        "Google OAuth token exchange failed; check the configured OAuth client and refresh credential.",
    )
}
// Preallocate for worst-case percent encoding so credential bytes never move to
// an abandoned allocation. Bytes owns this Zeroizing buffer through HTTP send.
fn form(client_id: &str, refresh_token: &Secret, client_secret: &Secret) -> Zeroizing<Vec<u8>> {
    let fields = [
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("refresh_token", refresh_token.expose()),
        ("client_secret", client_secret.expose()),
    ];
    let capacity = fields.iter().map(|(k, v)| k.len() + 2 + 3 * v.len()).sum();
    let mut body = Zeroizing::new(Vec::with_capacity(capacity));
    const HEX: &[u8] = b"0123456789ABCDEF";
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            body.push(b'&');
        }
        body.extend_from_slice(key.as_bytes());
        body.push(b'=');
        for byte in value.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                body.push(byte);
            } else {
                body.extend_from_slice(&[
                    b'%',
                    HEX[(byte >> 4) as usize],
                    HEX[(byte & 15) as usize],
                ]);
            }
        }
    }
    body
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: Zeroizing<String>,
    token_type: String,
    expires_in: u64,
}
async fn exchange(
    client_id: &str,
    refresh_token: &Secret,
    client_secret: &Secret,
    endpoint: Url,
) -> Result<Secret, ProviderError> {
    if client_id.is_empty()
        || client_id.len() > 1024
        || !valid_token(refresh_token.expose())
        || !valid_token(client_secret.expose())
    {
        return Err(auth_error());
    }
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| auth_error())?;
    tokio::time::timeout(Duration::from_secs(15), async {
        let body = bytes::Bytes::from_owner(form(client_id, refresh_token, client_secret));
        let mut response = client
            .post(endpoint)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| auth_error())?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > MAX_BODY as u64)
        {
            return Err(auth_error());
        }
        let mut buffer = Zeroizing::new(Vec::with_capacity(MAX_BODY));
        while let Some(chunk) = response.chunk().await.map_err(|_| auth_error())? {
            if chunk.len() > MAX_BODY - buffer.len() {
                return Err(auth_error());
            }
            buffer.extend_from_slice(&chunk);
        }
        let mut result: TokenResponse =
            serde_json::from_slice(&buffer).map_err(|_| auth_error())?;
        if !valid_token(&result.access_token)
            || !result.token_type.eq_ignore_ascii_case("Bearer")
            || result.expires_in == 0
        {
            return Err(auth_error());
        }
        Ok(Secret::new(std::mem::take(&mut *result.access_token)))
    })
    .await
    .map_err(|_| auth_error())?
}
pub(super) fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TOKEN && !value.chars().any(char::is_control)
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn mock(
        status: u16,
        headers: &str,
        body: &str,
    ) -> (Url, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint =
            Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
        let reply = format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        );
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut bytes = [0; 4096];
                let n = socket.read(&mut bytes).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&bytes[..n]);
                let text = String::from_utf8_lossy(&request);
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_owned)
                        })
                        .unwrap_or_else(|| "0".into())
                        .parse::<usize>()
                        .unwrap();
                    if body.len() >= length {
                        break;
                    }
                }
            }
            let _ = socket.write_all(reply.as_bytes()).await;
            String::from_utf8(request).unwrap()
        });
        (endpoint, task)
    }
    #[tokio::test]
    async fn refresh_exchanges_form_credentials_for_ephemeral_access_token() {
        let (endpoint,task)=mock(200,"",r#"{"access_token":"issued-token","token_type":"Bearer","expires_in":3600,"scope":"https://www.googleapis.com/auth/admin.directory.user.readonly"}"#).await;
        let access = exchange(
            "desktop.apps.googleusercontent.com",
            &Secret::new("refresh&secret".into()),
            &Secret::new("client+secret".into()),
            endpoint,
        )
        .await
        .unwrap();
        assert_eq!(access.expose(), "issued-token");
        let request = task.await.unwrap();
        assert!(request.starts_with("POST /token HTTP/1.1"));
        assert!(request.contains("application/x-www-form-urlencoded"));
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        let pairs: std::collections::BTreeMap<_, _> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(pairs["grant_type"], "refresh_token");
        assert_eq!(pairs["refresh_token"], "refresh&secret");
        assert_eq!(pairs["client_secret"], "client+secret");
        assert_eq!(pairs["client_id"], "desktop.apps.googleusercontent.com");
    }
    #[tokio::test]
    async fn untrusted_token_responses_are_bounded_redacted_and_never_redirected() {
        for (status, body) in [
            (400, "SECRET remote-message".into()),
            (302, "SECRET".into()),
            (200, "{bad SECRET".into()),
            (
                200,
                r#"{"access_token":"SECRET","token_type":"MAC","expires_in":3600}"#.into(),
            ),
            (
                200,
                r#"{"access_token":"","token_type":"Bearer","expires_in":3600}"#.into(),
            ),
            (200, "S".repeat(65537)),
        ] {
            let (endpoint, task) =
                mock(status, "Location: https://evil.invalid/token\r\n", &body).await;
            let result = exchange(
                "client",
                &Secret::new("refresh-secret".into()),
                &Secret::new("client-secret".into()),
                endpoint,
            )
            .await;
            let err = result.err().unwrap();
            assert!(!format!("{err:?}").contains("SECRET"));
            assert!(!err.message.contains("remote-message"));
            task.await.unwrap();
        }
    }
    #[tokio::test]
    async fn cancelling_refresh_drops_the_http_connection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint =
            Url::parse(&format!("http://{}/token", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            exchange(
                "client",
                &Secret::new("refresh-secret".into()),
                &Secret::new("client-secret".into()),
                endpoint,
            )
            .await
        });
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), socket.read(&mut request))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn refreshed_access_token_cannot_be_reflected_as_an_identity_or_email() {
        use permesh_provider_sdk::Provider;
        let (endpoint, task) = mock(
            200,
            "",
            r#"{"access_token":"SENTINELissued","token_type":"Bearer","expires_in":3600}"#,
        )
        .await;
        let access = exchange(
            "client",
            &Secret::new("refresh-secret".into()),
            &Secret::new("client-secret".into()),
            endpoint,
        )
        .await
        .unwrap();
        task.await.unwrap();
        let (endpoint,task)=mock(200,"",r#"{"users":[{"id":"SENTINELissued","customerId":"C123","primaryEmail":"normal@example.com","suspended":false,"archived":false},{"id":"123","customerId":"C123","primaryEmail":"SENTINELissued@example.com","suspended":false,"archived":false}]}"#).await;
        let mut provider =
            crate::GoogleProvider::new("directory".into(), "C123".into(), access).unwrap();
        provider.endpoint = endpoint;
        let snapshot = provider.discover().await.unwrap();
        task.await.unwrap();
        let output = serde_json::to_string(&snapshot).unwrap();
        assert!(!output.contains("SENTINELissued"));
        assert!(snapshot.accounts.is_empty() && snapshot.identities.is_empty());
        assert!(!snapshot.complete);
    }
    #[tokio::test]
    async fn redirect_cannot_send_credentials_to_a_second_origin() {
        let (destination, mut destination_task) = mock(
            200,
            "",
            r#"{"access_token":"redirected-token","token_type":"Bearer","expires_in":3600}"#,
        )
        .await;
        let (endpoint, task) = mock(307, &format!("Location: {destination}\r\n"), "").await;
        let result = exchange(
            "client",
            &Secret::new("refresh-secret".into()),
            &Secret::new("client-secret".into()),
            endpoint,
        )
        .await;
        assert!(result.is_err());
        task.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut destination_task)
                .await
                .is_err()
        );
        destination_task.abort();
    }
}
