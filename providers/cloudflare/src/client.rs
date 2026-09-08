// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use serde_json::Value;
use std::collections::BTreeSet;
#[derive(Default)]
pub(crate) struct Budget {
    requests: usize,
    rows: usize,
}
pub(crate) struct Listing {
    pub rows: Vec<Value>,
    pub failure: Option<&'static str>,
}
impl CloudflareProvider {
    pub(crate) async fn request(
        &self,
        path: &str,
        query: &[(&str, String)],
        budget: &mut Budget,
    ) -> Result<Value, ProviderError> {
        let mut url = self.origin.join(path).map_err(|_| error("configuration"))?;
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        for attempt in 0..3 {
            budget.requests += 1;
            if budget.requests > MAX_REQUESTS {
                return Err(error("limit"));
            }
            let (status, headers, bytes) = tokio::time::timeout(self.request_timeout, async {
                let mut response = self
                    .client
                    .get(url.clone())
                    .bearer_auth(self.token.expose())
                    .send()
                    .await
                    .map_err(|_| error("transport"))?;
                let status = response.status();
                let headers = response.headers().clone();
                if response
                    .content_length()
                    .is_some_and(|n| n > MAX_BODY as u64)
                {
                    return Err(error("limit"));
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|_| error("transport"))? {
                    if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
                        return Err(error("limit"));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok((status, headers, bytes))
            })
            .await
            .map_err(|_| error("timeout"))??;
            if status.as_u16() == 429 || status.is_server_error() {
                if attempt == 2 {
                    return Err(error(if status.as_u16() == 429 {
                        "rate_limit"
                    } else {
                        "transport"
                    }));
                }
                let delay = if let Some(v) = headers.get("retry-after") {
                    let seconds = v
                        .to_str()
                        .ok()
                        .and_then(|v| v.parse::<u64>().ok())
                        .ok_or_else(|| error("rate_limit"))?;
                    if seconds > 30 {
                        return Err(error("rate_limit"));
                    }
                    Duration::from_secs(seconds)
                } else if status.as_u16() == 429 {
                    return Err(error("rate_limit"));
                } else {
                    Duration::from_millis(100 * (1 << attempt))
                };
                tokio::time::sleep(delay).await;
                continue;
            }
            if !status.is_success() {
                return Err(error(match status.as_u16() {
                    401 => "unauthorized",
                    403 => "forbidden",
                    _ => "transport",
                }));
            }
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| error("malformed"))?;
            if value.get("success").and_then(Value::as_bool) != Some(true) {
                return Err(error("malformed"));
            }
            return Ok(value);
        }
        Err(error("transport"))
    }
    pub(crate) async fn listing(
        &self,
        path: &str,
        filters: &[(&str, String)],
        budget: &mut Budget,
    ) -> Listing {
        let mut rows = Vec::new();
        let mut pages = BTreeSet::new();
        let mut known_total = None;
        let result: Result<(), ProviderError> = async {
            for page in 1..=MAX_PAGES {
                let mut query = filters.to_vec();
                query.push(("per_page", "50".into()));
                query.push(("page", page.to_string()));
                let response = self.request(path, &query, budget).await?;
                let values = response
                    .get("result")
                    .and_then(Value::as_array)
                    .ok_or_else(|| error("malformed"))?;
                if values.len() > 50 {
                    return Err(error("limit"));
                }
                if !values.is_empty()
                    && !pages.insert(
                        values
                            .clone()
                            .into_iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>(),
                    )
                {
                    return Err(error("pagination"));
                }
                budget.rows = budget.rows.saturating_add(values.len());
                if budget.rows > MAX_ROWS {
                    return Err(error("limit"));
                }
                let more = match response.get("result_info") {
                    None => values.len() == 50,
                    Some(info) => {
                        if !info.is_object() {
                            return Err(error("pagination"));
                        }
                        for (key, expected) in [
                            ("page", page as u64),
                            ("per_page", 50),
                            ("count", values.len() as u64),
                        ] {
                            if info.get(key).is_some_and(|v| v.as_u64() != Some(expected)) {
                                return Err(error("pagination"));
                            }
                        }
                        if let Some(total) = info.get("total_count") {
                            let total = total.as_u64().ok_or_else(|| error("pagination"))?;
                            if known_total.is_some_and(|previous| previous != total) {
                                return Err(error("pagination"));
                            }
                            known_total = Some(total);
                            let received = rows.len() + values.len();
                            if total < received as u64
                                || (values.is_empty() && total > received as u64)
                            {
                                return Err(error("pagination"));
                            }
                            total > received as u64
                        } else {
                            values.len() == 50
                        }
                    }
                };
                rows.extend(values.iter().cloned());
                if !more {
                    return Ok(());
                }
            }
            Err(error("limit"))
        }
        .await;
        Listing {
            rows,
            failure: result.err().map(|e| category(&e.code)),
        }
    }
}
fn category(code: &str) -> &'static str {
    match code {
        "unauthorized" => "unauthorized",
        "forbidden" => "forbidden",
        "rate_limit" => "rate_limit",
        "timeout" => "timeout",
        "limit" => "limit",
        "pagination" => "pagination",
        _ => "malformed",
    }
}
