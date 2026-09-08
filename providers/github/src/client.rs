// SPDX-License-Identifier: MIT
//! Transport, bounded collection, retry policy, and same-endpoint pagination.
use crate::records::safe_segment;
use crate::{GithubProvider, MAX_BODY, MAX_PAGES, MAX_REQUESTS, MAX_ROWS, error};
use permesh_provider_sdk::ProviderError;
use reqwest::{Url, header::HeaderMap};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use time::{OffsetDateTime, format_description::well_known::Rfc2822};

impl GithubProvider {
    pub(super) fn url(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Url, ProviderError> {
        if segments.iter().any(|s| !safe_segment(s)) {
            return Err(error("malformed"));
        }
        let mut url = self.origin.clone();
        url.path_segments_mut()
            .map_err(|_| error("configuration"))?
            .clear()
            .extend(segments);
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        Ok(url)
    }

    pub(super) async fn get(
        &self,
        url: Url,
        budget: &mut Budget,
    ) -> Result<(HeaderMap, Value), ProviderError> {
        self.get_media(url, budget, "application/vnd.github+json")
            .await
    }

    pub(super) async fn get_media(
        &self,
        url: Url,
        budget: &mut Budget,
        accept: &str,
    ) -> Result<(HeaderMap, Value), ProviderError> {
        for attempt in 0..=2 {
            if budget.requests >= MAX_REQUESTS {
                return Err(error("limit"));
            }
            budget.requests += 1;
            let operation = async {
                let mut response = self
                    .client
                    .get(url.clone())
                    .bearer_auth(self.token.expose())
                    .header("Accept", accept)
                    .header("X-GitHub-Api-Version", "2026-03-10")
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
                // A 403 body is inspected only for documented rate-limit categories.
                // All error bodies are discarded and never included in diagnostics.
                if status != 200 && status != 403 {
                    return Ok((status, headers, None, false));
                }
                if response
                    .content_length()
                    .is_some_and(|n| n > MAX_BODY as u64)
                {
                    return Err(error("limit"));
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|_| error("transport"))? {
                    if chunk.len() > MAX_BODY.saturating_sub(bytes.len()) {
                        return Err(error("limit"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                if status == 403 {
                    let secondary = serde_json::from_slice::<Value>(&bytes)
                        .ok()
                        .and_then(|v| {
                            v.get("message").and_then(Value::as_str).map(|m| {
                                let message = m.to_ascii_lowercase();
                                message.contains("secondary rate limit")
                                    || message.contains("rate limit exceeded")
                                    || message.contains("abuse detection")
                            })
                        })
                        .unwrap_or(false);
                    return Ok((status, headers, None, secondary));
                }
                let value = serde_json::from_slice(&bytes).map_err(|_| error("malformed"))?;
                Ok((status, headers, Some(value), false))
            };
            let (status, headers, value, secondary) =
                tokio::time::timeout(self.request_timeout, operation)
                    .await
                    .map_err(|_| error("timeout"))??;
            if let Some(value) = value {
                return Ok((headers, value));
            }
            let limited = secondary
                || status == 429
                || (status == 403
                    && (headers.contains_key("retry-after")
                        || header(&headers, "x-ratelimit-remaining") == Some("0")));
            if attempt < 2 && (limited || (500..600).contains(&status)) {
                let delay = retry_delay(&headers, attempt, limited)?;
                tokio::time::sleep(delay).await;
                continue;
            }
            return Err(error(match status {
                401 => "unauthorized",
                403 if !limited => "forbidden",
                403 | 429 => "rate_limit",
                404 => "not_found",
                500..=599 => "unavailable",
                _ => "http",
            }));
        }
        Err(error("unavailable"))
    }

    pub(super) async fn object(
        &self,
        segments: &[&str],
        budget: &mut Budget,
    ) -> Result<Value, ProviderError> {
        let (_, value) = self.get(self.url(segments, &[])?, budget).await?;
        if !value.is_object() {
            return Err(error("malformed"));
        }
        Ok(value)
    }

    pub(super) async fn list(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
        budget: &mut Budget,
    ) -> List {
        let mut result = List::default();
        for page in 1..=MAX_PAGES {
            let page_string = page.to_string();
            let mut params = query.to_vec();
            params.extend([("per_page", "100"), ("page", page_string.as_str())]);
            let url = match self.url(segments, &params) {
                Ok(v) => v,
                Err(e) => {
                    result.error = Some(e);
                    break;
                }
            };
            let (headers, value) = match self.get(url.clone(), budget).await {
                Ok(v) => v,
                Err(e) => {
                    result.error = Some(e);
                    break;
                }
            };
            let Value::Array(items) = value else {
                result.error = Some(error("malformed"));
                break;
            };
            result.success = true;
            let length = items.len();
            if length > 100 {
                result.error = Some(error("limit"));
                break;
            }
            let remaining = MAX_ROWS.saturating_sub(budget.rows);
            let consumed = length.min(remaining);
            result.items.extend(items.into_iter().take(consumed));
            budget.rows += consumed;
            if consumed < length {
                result.error = Some(error("limit"));
                break;
            }
            match next_page(&url, &headers, length) {
                Ok(false) => break,
                Err(e) => {
                    result.error = Some(e);
                    break;
                }
                Ok(true) if page == MAX_PAGES => {
                    result.error = Some(error("limit"));
                    break;
                }
                Ok(true) => {}
            }
        }
        result
    }
}

#[derive(Default)]
pub(super) struct Budget {
    pub(super) requests: usize,
    pub(super) rows: usize,
}
#[derive(Default)]
pub(super) struct List {
    pub(super) items: Vec<Value>,
    pub(super) error: Option<ProviderError>,
    pub(super) success: bool,
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}
pub(super) fn retry_delay(
    headers: &HeaderMap,
    attempt: u32,
    limited: bool,
) -> Result<Duration, ProviderError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut seconds = if limited { 60 } else { 1_u64 << attempt };
    if let Some(value) = header(headers, "retry-after") {
        seconds = value
            .parse::<u64>()
            .ok()
            .or_else(|| {
                OffsetDateTime::parse(value, &Rfc2822)
                    .ok()
                    .map(|date| (date.unix_timestamp().max(0) as u64).saturating_sub(now.as_secs()))
            })
            .ok_or_else(|| error("rate_limit"))?;
    }
    if header(headers, "x-ratelimit-remaining") == Some("0") {
        let reset = header(headers, "x-ratelimit-reset")
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| error("rate_limit"))?;
        seconds = seconds.max(reset.saturating_sub(now.as_secs()));
    }
    if seconds > 60 {
        return Err(error("rate_limit"));
    }
    // Small bounded jitter; not used for security or identification.
    Ok(Duration::from_secs(seconds) + Duration::from_millis(u64::from(now.subsec_nanos() % 251)))
}
pub(super) fn next_page(
    current: &Url,
    headers: &HeaderMap,
    length: usize,
) -> Result<bool, ProviderError> {
    let Some(value) = headers.get("link") else {
        return Ok(length == 100);
    };
    let link = value.to_str().map_err(|_| error("pagination"))?;
    let mut has_next = false;
    for entry in link.split(',') {
        let (target, parameters) = entry
            .trim()
            .split_once('>')
            .ok_or_else(|| error("pagination"))?;
        let target = target
            .strip_prefix('<')
            .ok_or_else(|| error("pagination"))?;
        let rel = parameters
            .split(';')
            .find_map(|p| p.trim().strip_prefix("rel="));
        let rel = rel.ok_or_else(|| error("pagination"))?.trim_matches('"');
        if rel.split_whitespace().any(|r| r == "next") {
            let next = Url::parse(target).map_err(|_| error("pagination"))?;
            let current_params: BTreeMap<_, _> = current
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let mut expected = current_params;
            let page = expected
                .get("page")
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or_else(|| error("pagination"))?;
            expected.insert("page".into(), (page + 1).to_string());
            let pairs: Vec<_> = next
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let actual: BTreeMap<_, _> = pairs.iter().cloned().collect();
            if next.origin() != current.origin()
                || next.path() != current.path()
                || !next.username().is_empty()
                || next.password().is_some()
                || next.fragment().is_some()
                || pairs.len() != actual.len()
                || actual != expected
            {
                return Err(error("pagination"));
            }
            has_next = true;
        }
    }
    Ok(has_next)
}
