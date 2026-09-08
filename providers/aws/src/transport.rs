// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounded transport for official SDK requests; signing stays entirely in the SDK.
use crate::{ProviderError, error};
use aws_smithy_runtime_api::client::{
    http::{
        HttpConnector, HttpConnectorFuture, SharedHttpClient, SharedHttpConnector, http_client_fn,
    },
    orchestrator::{HttpRequest, HttpResponse},
    result::ConnectorError,
};
use aws_smithy_types::body::SdkBody;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BODY: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug)]
struct Transport {
    client: reqwest::Client,
    allowed: [String; 2],
    attempts: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
}
fn failure() -> ConnectorError {
    ConnectorError::other(
        Box::new(std::io::Error::other("AWS transport failed")),
        None,
    )
}
pub(crate) fn client(iam: String, sts: String) -> Result<SharedHttpClient, ProviderError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| error("configuration"))?;
    let transport = SharedHttpConnector::new(Transport {
        client,
        allowed: [iam, sts],
        attempts: Arc::new(AtomicUsize::new(0)),
        bytes: Arc::new(AtomicUsize::new(0)),
    });
    Ok(http_client_fn(move |_, _| transport.clone()))
}
impl HttpConnector for Transport {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let this = self.clone();
        HttpConnectorFuture::new(async move {
            if request.method() != "POST"
                || !this
                    .allowed
                    .iter()
                    .any(|url| request.uri() == url || request.uri() == format!("{url}/"))
                || this.attempts.fetch_add(1, Ordering::Relaxed) >= 600
            {
                return Err(failure());
            }
            let body = request
                .body()
                .bytes()
                .filter(|b| b.len() <= 64 * 1024)
                .ok_or_else(failure)?
                .to_vec();
            let iam_inventory = body
                .windows(b"Action=GetAccountAuthorizationDetails".len())
                .any(|v| v == b"Action=GetAccountAuthorizationDetails");
            let mut builder = this.client.post(request.uri()).body(body);
            for (name, value) in request.headers() {
                builder = builder.header(name, value);
            }
            let mut response = builder.send().await.map_err(|_| failure())?;
            if response
                .content_length()
                .is_some_and(|n| n > MAX_BODY as u64)
            {
                return Err(failure());
            }
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
                if chunk.len() > MAX_BODY.saturating_sub(bytes.len())
                    || this
                        .bytes
                        .fetch_add(chunk.len(), Ordering::Relaxed)
                        .saturating_add(chunk.len())
                        > MAX_TOTAL_BODY
                {
                    return Err(failure());
                }
                bytes.extend_from_slice(&chunk);
            }
            if status == 200 && iam_inventory && !explicit_pagination(&bytes) {
                return Err(failure());
            }
            let mut result = HttpResponse::new(
                status.try_into().map_err(|_| failure())?,
                SdkBody::from(bytes),
            );
            for (name, value) in &headers {
                result
                    .headers_mut()
                    .try_insert(
                        name.as_str().to_owned(),
                        value.to_str().map_err(|_| failure())?.to_owned(),
                    )
                    .map_err(|_| failure())?;
            }
            Ok(result)
        })
    }
}

// The generated SDK defaults an absent IsTruncated to false. Require an explicit
// signal at its documented location before treating any inventory page as complete.
fn explicit_pagination(bytes: &[u8]) -> bool {
    use xmlparser::{ElementEnd, Token, Tokenizer};
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut stack = Vec::new();
    let mut found = 0;
    let mut value = String::new();
    for token in Tokenizer::from(text) {
        match token {
            Ok(Token::ElementStart { local, .. }) => {
                stack.push(local.as_str());
                if stack.len() > 64 {
                    return false;
                }
            }
            Ok(Token::Text { text }) => {
                if stack
                    == [
                        "GetAccountAuthorizationDetailsResponse",
                        "GetAccountAuthorizationDetailsResult",
                        "IsTruncated",
                    ]
                {
                    value.push_str(text.as_str())
                }
            }
            Ok(Token::ElementEnd { end, .. }) => {
                if matches!(end, ElementEnd::Open) {
                    continue;
                }
                if let ElementEnd::Close(_, local) = end
                    && stack.last().copied() != Some(local.as_str())
                {
                    return false;
                }
                if stack
                    == [
                        "GetAccountAuthorizationDetailsResponse",
                        "GetAccountAuthorizationDetailsResult",
                        "IsTruncated",
                    ]
                {
                    found += 1;
                    if !matches!(value.trim(), "true" | "false") {
                        return false;
                    }
                    value.clear();
                }
                stack.pop();
            }
            Ok(
                Token::DtdStart { .. } | Token::EmptyDtd { .. } | Token::EntityDeclaration { .. },
            )
            | Err(_) => return false,
            _ => {}
        }
    }
    found == 1 && stack.is_empty()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pagination_guard_requires_matching_elements_and_one_explicit_boolean() {
        let valid = "<GetAccountAuthorizationDetailsResponse><GetAccountAuthorizationDetailsResult><IsTruncated>false</IsTruncated></GetAccountAuthorizationDetailsResult></GetAccountAuthorizationDetailsResponse>";
        assert!(explicit_pagination(valid.as_bytes()));
        for invalid in [
            valid.replace("</IsTruncated>", "</Other>"),
            valid.replace("false", "invalid"),
            valid.replace(
                "<IsTruncated>false</IsTruncated>",
                "<IsTruncated>false</IsTruncated><IsTruncated>true</IsTruncated>",
            ),
        ] {
            assert!(!explicit_pagination(invalid.as_bytes()));
        }
    }
}
