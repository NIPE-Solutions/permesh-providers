// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use super::*;
use permesh_provider_protocol::negotiated::{DiscoveryDecoder, HealthDecoder};
use permesh_provider_sdk::Provider;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn mock(responses: Vec<(u16, Value)>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/users", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 8192];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            let body = body.to_string();
            let reply = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    (endpoint, task)
}
async fn serve_mock<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: R,
    writer: W,
    endpoint: String,
) -> Result<(), ProtocolFailure> {
    permesh_native_runtime::serve::<Google, _, _, _, _, _>(
        reader,
        writer,
        |id, config, credentials| async move {
            let mut provider =
                GoogleProvider::new(id, config.customer_id, secret(credentials.token)?)?;
            provider.endpoint = reqwest::Url::parse(&endpoint).unwrap();
            Ok(provider)
        },
    )
    .await
}
fn input(method: &str) -> String {
    format!(
        "{}\n{}\n",
        json!({"protocol_version":1,"id":"handshake","method":"handshake","operation":method,"instance":"directory"}),
        json!({"protocol_version":1,"id":method,"method":method,"configuration":{"customer_id":"C123"},"credentials":{"token":"SENTINEL-secret"}})
    )
}
async fn exchange(endpoint: String, method: &str) -> (Result<(), ProtocolFailure>, Vec<u8>) {
    let (mut host, peer) = tokio::io::duplex(128 * 1024);
    let (reader, writer) = tokio::io::split(peer);
    let session = tokio::spawn(serve_mock(reader, writer, endpoint));
    host.write_all(input(method).as_bytes()).await.unwrap();
    let mut output = Vec::new();
    host.read_to_end(&mut output).await.unwrap();
    (session.await.unwrap(), output)
}
fn user(id: &str) -> Value {
    json!({"id":id,"customerId":"C123","primaryEmail":format!("{id}@example.com"),"suspended":false,"archived":false})
}
#[tokio::test]
async fn discovery_wire_preserves_native_accounts_identities_status_and_partial_pages() {
    for partial in [false, true] {
        let responses = vec![
            (200, json!({"users":[user("1")],"nextPageToken":"next"})),
            if partial {
                (403, json!({"error":"SENTINEL-secret"}))
            } else {
                (200, json!({"users":[user("2")]}))
            },
        ];
        let (endpoint, task) = mock(responses.clone()).await;
        let mut provider = GoogleProvider::new(
            "directory".into(),
            "C123".into(),
            Secret::new("SENTINEL-secret".into()),
        )
        .unwrap();
        provider.endpoint = reqwest::Url::parse(&endpoint).unwrap();
        let expected = provider.discover().await.unwrap();
        task.await.unwrap();
        let (endpoint, task) = mock(responses).await;
        let (result, output) = exchange(endpoint, "discover").await;
        assert!(result.is_ok());
        assert!(!String::from_utf8_lossy(&output).contains("SENTINEL-secret"));
        let mut decoder = DiscoveryDecoder::new(
            "google",
            "directory",
            Some(&crate::provider_metadata().capabilities),
        )
        .unwrap();
        for frame in output.split_inclusive(|b| *b == b'\n') {
            decoder.push_frame(frame).unwrap();
        }
        let actual = decoder.finish().unwrap();
        assert_eq!(json!(actual.accounts), json!(expected.accounts));
        assert_eq!(json!(actual.identities), json!(expected.identities));
        assert_eq!(actual.complete, !partial);
        assert_eq!(actual.identities[0].id, "google:C123:1");
        task.await.unwrap();
    }
}
#[tokio::test]
async fn health_and_failures_cross_decode_and_redact_remote_errors() {
    let (endpoint, task) = mock(vec![(200, json!({"users":[user("1")]}))]).await;
    let (result, output) = exchange(endpoint, "check").await;
    assert!(result.is_ok());
    let mut decoder = HealthDecoder::new(
        "google",
        "directory",
        Some(&crate::provider_metadata().capabilities),
    )
    .unwrap();
    for frame in output.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    assert!(!decoder.finish().unwrap().limitations.is_empty());
    task.await.unwrap();
    for (status, code) in [(401, "authentication"), (403, "permission_denied")] {
        let (endpoint, task) = mock(vec![(status, json!({"error":"SENTINEL-secret"}))]).await;
        let (result, output) = exchange(endpoint, "check").await;
        assert!(result.is_err());
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains(code));
        assert!(!output.contains("SENTINEL-secret"));
        task.await.unwrap();
    }
}
#[tokio::test]
async fn cancellation_drops_directory_request_and_does_not_echo_credentials() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/users", listener.local_addr().unwrap());
    let (mut host, peer) = tokio::io::duplex(8192);
    let (reader, writer) = tokio::io::split(peer);
    let session = tokio::spawn(serve_mock(reader, writer, endpoint));
    host.write_all(input("discover").as_bytes()).await.unwrap();
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut buf = [0; 8192];
    assert!(socket.read(&mut buf).await.unwrap() > 0);
    host.write_all(b"{\"protocol_version\":1,\"id\":\"cancel\",\"method\":\"cancel\"}\n")
        .await
        .unwrap();
    let mut output = String::new();
    host.read_to_string(&mut output).await.unwrap();
    assert!(session.await.unwrap().is_ok());
    assert!(output.contains("cancelled"));
    assert!(!output.contains("SENTINEL-secret"));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), socket.read(&mut buf))
            .await
            .unwrap()
            .unwrap(),
        0
    );
}
#[test]
fn schemas_require_exact_auth_credentials_and_reject_overrides_duplicates_and_null() {
    let valid = json!({"protocol_version":1,"id":"check","method":"check","configuration":{"customer_id":"C123","auth_mode":"refresh_token","client_id":"desktop.apps.googleusercontent.com"},"credentials":{"refresh_token":"refresh","client_secret":"secret"}});
    assert!(
        permesh_native_runtime::validate_request::<Google>(valid.to_string().as_bytes()).is_ok()
    );
    for (pointer, value) in [
        (
            "/configuration/token_endpoint",
            json!("https://evil.invalid"),
        ),
        ("/configuration/customer_id", json!("my_customer")),
        ("/configuration/client_id", json!(null)),
        ("/credentials/token", json!("extra")),
        ("/credentials/client_secret", json!(null)),
        ("/credentials/refresh_token", json!("x".repeat(16385))),
    ] {
        let mut request = valid.clone();
        let (object, key) = pointer.rsplit_once('/').unwrap();
        request
            .pointer_mut(object)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        assert!(
            permesh_native_runtime::validate_request::<Google>(request.to_string().as_bytes())
                .is_err()
        );
    }
    let duplicate = valid.to_string().replace(
        "\"refresh_token\":\"refresh\"",
        "\"refresh_token\":\"refresh\",\"refresh_token\":\"second\"",
    );
    assert!(permesh_native_runtime::validate_request::<Google>(duplicate.as_bytes()).is_err());
}
#[test]
fn setup_refresh_mode_only_requests_its_required_named_credentials() {
    let spec = setup::spec();
    spec.validate().unwrap();
    let answers = std::collections::BTreeMap::from([
        ("customer_id".into(), json!("C123")),
        ("auth_mode".into(), json!("refresh_token")),
    ]);
    let questions = spec.questions(&answers).unwrap();
    let keys: Vec<_> = questions.iter().map(|q| q.field.key.as_str()).collect();
    assert!(
        keys.contains(&"client_id")
            && keys.contains(&"refresh_token")
            && keys.contains(&"client_secret")
    );
    assert!(!keys.contains(&"token"));
}
