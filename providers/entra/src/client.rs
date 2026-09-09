// SPDX-License-Identifier: MIT
//! Bounded Graph GET transport and origin/path/query constrained next links.
use crate::{EntraProvider, MAX_BODY, MAX_PAGES, MAX_REQUESTS, MAX_ROWS, error, records::Object};
use permesh_provider_sdk::ProviderError;
use reqwest::Url;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
#[derive(Default)]
pub(crate) struct Budget {
    requests: usize,
    rows: usize,
}
#[derive(Deserialize)]
pub(crate) struct Page {
    pub value: Vec<Object>,
    #[serde(rename = "@odata.nextLink")]
    pub next: Option<String>,
}
pub(crate) struct Listing {
    pub rows: Vec<Object>,
    pub failure: Option<&'static str>,
}
impl EntraProvider {
    pub(crate) fn url(&self, path: &str, select: Option<&str>) -> Result<Url, ProviderError> {
        let mut url = self.origin.join(path).map_err(|_| error("configuration"))?;
        if let Some(select) = select {
            url.query_pairs_mut()
                .append_pair("$select", select)
                .append_pair("$top", "100");
        }
        Ok(url)
    }
    pub(crate) async fn request(
        &self,
        url: &Url,
        budget: &mut Budget,
    ) -> Result<Page, ProviderError> {
        for attempt in 0..3 {
            budget.requests = budget.requests.saturating_add(1);
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
                    .is_some_and(|size| size > MAX_BODY as u64)
                {
                    return Err(error("limit"));
                }
                let mut bytes = vec![];
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
                let delay = if let Some(header) = headers.get("retry-after") {
                    let seconds = header
                        .to_str()
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|s| *s <= 10)
                        .ok_or_else(|| error("rate_limit"))?;
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
            // Decode strings before checking reflection so JSON escapes cannot bypass it.
            let value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| error("malformed"))?;
            if reflects(&value, self.token.expose()) {
                return Err(error("malformed"));
            }
            // Parse original bytes again to preserve typed duplicate-field rejection.
            let mut page: Page = serde_json::from_slice(&bytes).map_err(|_| error("malformed"))?;
            if page.value.len() > 999 {
                return Err(error("limit"));
            }
            for row in &mut page.value {
                row.validate()?;
            }
            return Ok(page);
        }
        Err(error("transport"))
    }
    pub(crate) fn next_url(&self, initial: &Url, next: &str) -> Result<Url, ProviderError> {
        if next.len() > 16384 {
            return Err(error("pagination"));
        }
        let url = Url::parse(next).map_err(|_| error("pagination"))?;
        if url.origin() != initial.origin()
            || url.path() != initial.path()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(error("pagination"));
        }
        let mut query = BTreeMap::new();
        for (k, v) in url.query_pairs() {
            if !matches!(k.as_ref(), "$select" | "$top" | "$skiptoken")
                || query.insert(k.into_owned(), v.into_owned()).is_some()
            {
                return Err(error("pagination"));
            }
        }
        if query.contains_key("$select") && !initial.query_pairs().any(|(key, _)| key == "$select")
        {
            return Err(error("pagination"));
        }
        for (k, v) in initial.query_pairs() {
            if query.get(k.as_ref()).map(String::as_str) != Some(v.as_ref()) {
                return Err(error("pagination"));
            }
        }
        if query.get("$skiptoken").is_none_or(|s| s.is_empty())
            || query
                .get("$top")
                .is_some_and(|s| s.parse::<usize>().ok().is_none_or(|n| n == 0 || n > 999))
        {
            return Err(error("pagination"));
        }
        Ok(url)
    }
    pub(crate) async fn listing(&self, initial: Url, budget: &mut Budget) -> Listing {
        let mut rows = vec![];
        let mut next = initial.clone();
        let mut seen_urls = BTreeSet::new();
        let mut seen_ids = BTreeSet::new();
        let result: Result<(), ProviderError> = async {
            for _ in 0..MAX_PAGES {
                if !seen_urls.insert(next.to_string()) {
                    return Err(error("pagination"));
                }
                let page = self.request(&next, budget).await?;
                for row in page.value {
                    budget.rows = budget.rows.saturating_add(1);
                    if budget.rows > MAX_ROWS {
                        return Err(error("limit"));
                    }
                    if !seen_ids.insert(row.id.clone()) {
                        return Err(error("pagination"));
                    }
                    rows.push(row);
                }
                match page.next {
                    Some(url) => next = self.next_url(&initial, &url)?,
                    None => return Ok(()),
                }
            }
            Err(error("limit"))
        }
        .await;
        Listing {
            rows,
            failure: result.err().map(|e| match e.code.as_str() {
                "unauthorized" => "unauthorized",
                "forbidden" => "forbidden",
                "rate_limit" => "rate_limit",
                "limit" => "limit",
                "pagination" => "pagination",
                "malformed" => "malformed",
                "timeout" => "timeout",
                _ => "transport",
            }),
        }
    }
    pub(crate) async fn prove_tenant(&self, budget: &mut Budget) -> Result<(), ProviderError> {
        let mut url = self
            .origin
            .join("organization")
            .map_err(|_| error("configuration"))?;
        url.query_pairs_mut().append_pair("$select", "id");
        let page = self.request(&url, budget).await?;
        if page.next.is_some() || page.value.len() != 1 || page.value[0].id != self.tenant_id {
            return Err(error("scope"));
        }
        Ok(())
    }
}

fn reflects(value: &serde_json::Value, token: &str) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains(token),
        serde_json::Value::Array(rows) => rows.iter().any(|v| reflects(v, token)),
        serde_json::Value::Object(map) => map
            .iter()
            .any(|(key, value)| key.contains(token) || reflects(value, token)),
        _ => false,
    }
}
