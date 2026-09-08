// SPDX-License-Identifier: MIT OR Apache-2.0
//! Read-only Cloudflare IAM assignment evidence, never an effective-access evaluator.
mod client;
pub mod protocol;
mod records;
use permesh_core::Snapshot;
use permesh_provider_sdk::{Capability, Health, Metadata, Provider, ProviderError, ProviderFuture};
use permesh_secrets::Secret;
use reqwest::{Client, Url};
use std::time::Duration;

const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_PAGES: usize = 100;
const MAX_ROWS: usize = 20_000;
const MAX_REQUESTS: usize = 2_000;
const VISIBILITY: &[&str] = &[
    "Cloudflare token scope limits visibility; absence does not establish absence of access. Collection is not transactional.",
    "Grants are observed policy assignments with unknown privilege, not evaluated effective access. Denies and unsupported scopes suppress potentially misleading access paths.",
    "Wildcard policy scopes are separate evidence resources, not expanded account or zone grants. Legacy roles without policy scopes are not normalized.",
    "Pending memberships, authoritative identities, verified email, organization policies, API-token principals, Zero Trust Access rules and non-zone product resources are not enumerated.",
];
pub struct CloudflareProvider {
    id: String,
    account_id: String,
    token: Secret,
    client: Client,
    origin: Url,
    request_timeout: Duration,
    operation_timeout: Duration,
}
impl CloudflareProvider {
    pub fn new(id: String, account_id: String, token: Secret) -> Result<Self, ProviderError> {
        if !valid_instance(&id)
            || !records::native_id(&account_id)
            || token.expose().is_empty()
            || token.expose().len() > 16 * 1024
            || !token.expose().bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(error("configuration"));
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("permesh-cloudflare/0.1")
            .build()
            .map_err(|_| error("configuration"))?;
        Ok(Self {
            id,
            account_id,
            token,
            client,
            origin: Url::parse("https://api.cloudflare.com/client/v4/")
                .map_err(|_| error("configuration"))?,
            request_timeout: Duration::from_secs(15),
            operation_timeout: Duration::from_secs(120),
        })
    }
}
pub fn valid_instance(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "cloudflare".into(),
        capabilities: vec![
            Capability::Accounts,
            Capability::Resources,
            Capability::Groups,
            Capability::Memberships,
            Capability::Grants,
        ],
    }
}
impl Provider for CloudflareProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            tokio::time::timeout(self.operation_timeout, async {
            let mut budget = client::Budget::default();
            let account = self.request(&format!("accounts/{}",self.account_id), &[], &mut budget).await?;
            self.account(&account)?;
            let members = self.request(&format!("accounts/{}/members",self.account_id), &[("per_page","5".into()),("page","1".into())], &mut budget).await?;
            let rows=members.get("result").and_then(serde_json::Value::as_array).filter(|r| r.len() <= 5).ok_or_else(|| error("malformed"))?;
            for row in rows { records::member(row)?; }
            Ok(Health { message: "Cloudflare account and bounded member-list probes succeeded; discovery permissions are not established.".into(), limitations: VISIBILITY.iter().map(|s| (*s).into()).collect() })
        }).await.map_err(|_| error("timeout"))?
        })
    }
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        Box::pin(async {
            tokio::time::timeout(self.operation_timeout, self.collect())
                .await
                .map_err(|_| error("timeout"))?
        })
    }
}
fn error(code: &str) -> ProviderError {
    ProviderError::new(
        code,
        match code {
            "configuration" => "Invalid Cloudflare provider configuration.",
            "unauthorized" => "Cloudflare rejected the credential.",
            "forbidden" => {
                "Cloudflare denied access; check Account Settings Read and Zone Read scope."
            }
            "rate_limit" => "Cloudflare rate limit exceeds the bounded retry budget; retry later.",
            "timeout" => "Cloudflare collection exceeded its deadline.",
            "limit" => "Cloudflare collection exceeded a response, record, page or request limit.",
            "pagination" => "Cloudflare returned inconsistent pagination.",
            "scope" => "Cloudflare returned a resource outside the configured account.",
            "malformed" => "Cloudflare returned malformed or conflicting access metadata.",
            "unsupported_policy" => {
                "Cloudflare deny, legacy or unsupported policy semantics prevented safe normalization."
            }
            _ => "Cloudflare request could not be completed.",
        },
    )
}
#[cfg(test)]
mod tests;
