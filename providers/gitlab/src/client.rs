// SPDX-License-Identifier: MIT
use crate::*;
use reqwest::header::HeaderMap;
use serde_json::Value;
#[derive(Default)]
pub(crate) struct Budget {
    requests: usize,
    rows: usize,
    bytes: usize,
}
pub(crate) struct Listing {
    pub rows: Vec<Value>,
    pub failure: Option<ProviderError>,
}
impl GitlabProvider {
    fn url(&self, path: &str, query: &[(&str, String)]) -> Result<Url, ProviderError> {
        let mut url = self
            .origin
            .join(&format!("api/v4/{path}"))
            .map_err(|_| error("configuration"))?;
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(key, value)| (*key, value)));
        Ok(url)
    }
    pub(crate) async fn request(
        &self,
        path: &str,
        query: &[(&str, String)],
        budget: &mut Budget,
    ) -> Result<(Value, HeaderMap), ProviderError> {
        let url = self.url(path, query)?;
        for attempt in 0..3 {
            budget.requests += 1;
            if budget.requests > MAX_REQUESTS {
                return Err(error("limit"));
            }
            let (status, headers, bytes) = tokio::time::timeout(self.request_timeout, async {
                let mut response = self
                    .client
                    .get(url.clone())
                    .header("PRIVATE-TOKEN", self.token.expose())
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
            budget.bytes = budget.bytes.saturating_add(bytes.len());
            if budget.bytes > MAX_TOTAL_BODY {
                return Err(error("limit"));
            }
            if status.as_u16() == 429 || status.is_server_error() {
                if attempt == 2 {
                    return Err(error(if status.as_u16() == 429 {
                        "rate_limit"
                    } else {
                        "transport"
                    }));
                }
                let delay = if let Some(value) = headers.get("retry-after") {
                    let delay = value
                        .to_str()
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|v| *v <= 5)
                        .ok_or_else(|| error("rate_limit"))?;
                    Duration::from_secs(delay)
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
                    403 | 404 => "forbidden",
                    _ => "transport",
                }));
            }
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| error("malformed"))?;
            if reflects(&value, self.token.expose()) {
                return Err(error("malformed"));
            }
            return Ok((value, headers));
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
        let mut ids = BTreeSet::new();
        let result: Result<(), ProviderError> = async {
            for page in 1..=MAX_PAGES {
                let mut query = filters.to_vec();
                query.extend([("per_page", "100".into()), ("page", page.to_string())]);
                let (value, headers) = self.request(path, &query, budget).await?;
                let items = value
                    .as_array()
                    .filter(|v| v.len() <= 100)
                    .ok_or_else(|| error("malformed"))?;
                budget.rows = budget.rows.saturating_add(items.len());
                if budget.rows > MAX_ROWS {
                    return Err(error("limit"));
                }
                let next = next_page(&headers, &self.url(path, &query)?, page, items.len())?;
                for item in items {
                    let id = records::native_id(item)?;
                    if !ids.insert(id) {
                        rows.clear();
                        return Err(error("malformed"));
                    }
                }
                rows.extend(items.iter().cloned());
                if !next {
                    return Ok(());
                }
                if items.is_empty() {
                    return Err(error("pagination"));
                }
            }
            Err(error("limit"))
        }
        .await;
        Listing {
            rows,
            failure: result.err(),
        }
    }
}
fn number(headers: &HeaderMap, name: &str) -> Result<Option<Option<usize>>, ProviderError> {
    let values: Vec<_> = headers.get_all(name).iter().collect();
    if values.len() > 1 {
        return Err(error("pagination"));
    }
    let Some(value) = values.first() else {
        return Ok(None);
    };
    let text = value.to_str().map_err(|_| error("pagination"))?;
    if text.is_empty() {
        return Ok(Some(None));
    }
    let n = text
        .parse::<usize>()
        .ok()
        .filter(|n| n.to_string() == text)
        .ok_or_else(|| error("pagination"))?;
    Ok(Some(Some(n)))
}
fn next_page(
    headers: &HeaderMap,
    current: &Url,
    page: usize,
    count: usize,
) -> Result<bool, ProviderError> {
    if number(headers, "x-page")?.is_some_and(|v| v != Some(page))
        || number(headers, "x-per-page")?.is_some_and(|v| v != Some(100))
    {
        return Err(error("pagination"));
    }
    let header = number(headers, "x-next-page")?
        .map(|v| match v {
            None => Ok(false),
            Some(next) if next == page + 1 => Ok(true),
            _ => Err(error("pagination")),
        })
        .transpose()?;
    let total = number(headers, "x-total-pages")?
        .map(|v| match v {
            Some(0) if page == 1 && count == 0 => Ok(false),
            Some(total) if total >= page => Ok(total > page),
            _ => Err(error("pagination")),
        })
        .transpose()?;
    let mut link_next = None;
    for header in headers.get_all("link").iter() {
        let text = header.to_str().map_err(|_| error("pagination"))?;
        for item in text.split(',') {
            let (destination, relation) = item
                .trim()
                .split_once(';')
                .ok_or_else(|| error("pagination"))?;
            let url = Url::parse(
                destination
                    .trim()
                    .strip_prefix('<')
                    .and_then(|s| s.strip_suffix('>'))
                    .ok_or_else(|| error("pagination"))?,
            )
            .map_err(|_| error("pagination"))?;
            let rel = relation
                .trim()
                .strip_prefix("rel=\"")
                .and_then(|s| s.strip_suffix('"'))
                .ok_or_else(|| error("pagination"))?;
            if !matches!(rel, "next" | "prev" | "first" | "last")
                || url.origin() != current.origin()
                || url.path() != current.path()
                || url.username() != current.username()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err(error("pagination"));
            }
            let pairs: Vec<_> = url.query_pairs().collect();
            let mut seen = BTreeSet::new();
            if pairs.iter().any(|(k, _)| !seen.insert(k.to_string())) {
                return Err(error("pagination"));
            }
            let expected: std::collections::BTreeMap<_, _> = current
                .query_pairs()
                .filter(|(k, _)| k != "page")
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let actual: std::collections::BTreeMap<_, _> = pairs
                .iter()
                .filter(|(k, _)| k != "page")
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let linked_page = pairs
                .iter()
                .find(|(k, _)| k == "page")
                .and_then(|(_, v)| v.parse::<usize>().ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| error("pagination"))?;
            if expected != actual {
                return Err(error("pagination"));
            }
            if rel == "next" {
                if link_next.is_some() || linked_page != page + 1 {
                    return Err(error("pagination"));
                }
                link_next = Some(true);
            }
        }
    }
    if matches!((header,link_next),(Some(a),Some(b)) if a!=b) {
        return Err(error("pagination"));
    }
    let more = header.or(link_next);
    if matches!((more,total),(Some(a),Some(b)) if a!=b) {
        return Err(error("pagination"));
    }
    Ok(more.or(total).unwrap_or(count == 100))
}

fn reflects(value: &Value, secret: &str) -> bool {
    match value {
        Value::String(text) => text.contains(secret),
        Value::Array(items) => items.iter().any(|v| reflects(v, secret)),
        Value::Object(items) => items
            .iter()
            .any(|(key, value)| key.contains(secret) || reflects(value, secret)),
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn pagination_links_cannot_change_endpoint_filters_origin_or_skip_pages() {
        let current=Url::parse("https://gitlab.example/api/v4/groups/10/projects?with_shared=false&include_subgroups=false&per_page=100&page=1").unwrap();
        for next in [
            "https://gitlab.example/api/v4/user?with_shared=false&include_subgroups=false&per_page=100&page=2",
            "https://gitlab.example/api/v4/groups/10/projects?with_shared=true&include_subgroups=false&per_page=100&page=2",
            "https://gitlab.example/api/v4/groups/10/projects?with_shared=false&include_subgroups=false&per_page=100&page=3",
            "https://gitlab.example/api/v4/groups/10/projects?with_shared=false&include_subgroups=false&per_page=100&page=2&page=2",
            "https://other.example/api/v4/groups/10/projects?with_shared=false&include_subgroups=false&per_page=100&page=2",
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("link", format!("<{next}>; rel=\"next\"").parse().unwrap());
            assert!(next_page(&headers, &current, 1, 1).is_err());
        }
        let mut headers = HeaderMap::new();
        headers.insert("link","<https://gitlab.example/api/v4/groups/10/projects?with_shared=false&include_subgroups=false&per_page=100&page=2>; rel=\"next\"".parse().unwrap());
        assert!(next_page(&headers, &current, 1, 1).unwrap());
        headers.insert("x-next-page", "".parse().unwrap());
        assert!(next_page(&headers, &current, 1, 1).is_err());
    }
}
