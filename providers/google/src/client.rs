// SPDX-License-Identifier: MIT
use super::{GoogleProvider, error};
use permesh_provider_sdk::ProviderError;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::{OffsetDateTime, format_description::well_known::Rfc2822};
const MAX_BODY: usize = 2 * 1024 * 1024;
impl GoogleProvider {
    pub(super) async fn page(
        &self,
        token: Option<&str>,
        size: usize,
    ) -> Result<Value, ProviderError> {
        let mut url = self.endpoint.clone();
        url.query_pairs_mut().extend_pairs([
            ("customer", self.customer_id.as_str()),
            ("maxResults", &size.to_string()),
            ("projection", "basic"),
            ("viewType", "admin_view"),
            (
                "fields",
                "nextPageToken,users(id,customerId,primaryEmail,suspended,archived)",
            ),
        ]);
        if let Some(token) = token {
            url.query_pairs_mut().append_pair("pageToken", token);
        }
        for attempt in 0..=2 {
            let operation = async {
                let mut response = self
                    .client
                    .get(url.clone())
                    .bearer_auth(self.token.expose())
                    .header("Accept", "application/json")
                    .send()
                    .await
                    .map_err(|err| {
                        error(if err.is_timeout() {
                            "timeout"
                        } else {
                            "transport"
                        })
                    })?;
                let status = response.status().as_u16();
                let headers = response.headers().clone();
                if response
                    .content_length()
                    .is_some_and(|length| length > MAX_BODY as u64)
                {
                    return Err(error("limit"));
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|err| {
                    error(if err.is_timeout() {
                        "timeout"
                    } else {
                        "transport"
                    })
                })? {
                    if chunk.len() > MAX_BODY.saturating_sub(bytes.len()) {
                        return Err(error("limit"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok((status, headers, bytes))
            };
            let (status, headers, bytes) = tokio::time::timeout(self.request_timeout, operation)
                .await
                .map_err(|_| error("timeout"))??;
            if (200..300).contains(&status) {
                return serde_json::from_slice(&bytes).map_err(|_| error("malformed"));
            }
            let limited = status == 429 || (status == 403 && quota(&bytes));
            if attempt < 2 && (limited || (500..600).contains(&status)) {
                tokio::time::sleep(retry_delay(&headers, attempt)?).await;
                continue;
            }
            return Err(error(match status {
                401 => "unauthorized",
                403 if !limited => "forbidden",
                403 | 429 => "rate_limit",
                _ => "http",
            }));
        }
        Err(error("http"))
    }
}
fn quota(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| {
            v.pointer("/error/errors")
                .and_then(Value::as_array)
                .map(|errors| {
                    errors.iter().any(|e| {
                        matches!(
                            e.get("reason").and_then(Value::as_str),
                            Some("rateLimitExceeded" | "userRateLimitExceeded" | "quotaExceeded")
                        )
                    })
                })
        })
        .unwrap_or(false)
}
fn retry_delay(headers: &HeaderMap, attempt: u32) -> Result<Duration, ProviderError> {
    let jitter = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
        % 101;
    let backoff = Duration::from_millis(1000 * (1u64 << attempt) + jitter);
    let Some(header) = headers.get("retry-after") else {
        return Ok(backoff);
    };
    let value = header.to_str().map_err(|_| error("rate_limit"))?;
    let delay = if !value.is_empty() && value.bytes().all(|c| c.is_ascii_digit()) {
        Duration::from_secs(value.parse().map_err(|_| error("rate_limit"))?)
    } else {
        let deadline = OffsetDateTime::parse(value, &Rfc2822).map_err(|_| error("rate_limit"))?;
        let remaining = deadline - OffsetDateTime::now_utc();
        Duration::try_from(remaining).unwrap_or_default()
    };
    if delay > Duration::from_secs(5) {
        return Err(error("rate_limit"));
    }
    Ok(delay.max(backoff))
}
