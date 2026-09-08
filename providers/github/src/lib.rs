// SPDX-License-Identifier: MIT
//! Bounded, read-only GitHub.com discovery. No provider-controlled URL is requested.
mod client;
mod discovery;
pub use discovery::provider_metadata;
mod records;
use permesh_provider_sdk::ProviderError;
use permesh_secrets::Secret;
use records::safe_segment;
use reqwest::{Client, Url};
use std::time::Duration;

const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_PAGES: usize = 100;
const MAX_ROWS: usize = 20_000;
const MAX_REQUESTS: usize = 2_000;
const VISIBILITY: &[&str] = &[
    "GitHub visibility is limited by token permissions, selected repositories, organization membership, SSO authorization, and secret teams; absence is not proof of no access.",
    "Repository collaborator roles describe effective access, including inherited team, organization, and enterprise access; they are not evidence of direct grants.",
    "Team memberships and repository permissions can include nested-team inheritance; the API observations do not establish direct assignment.",
    "Public email is not verified and is excluded. Pending invitations, enterprise policy, organization base-permission settings, and non-repository resources are not enumerated. Collection is not transactional.",
];

pub struct GithubProvider {
    id: String,
    organizations: Vec<String>,
    token: Secret,
    client: Client,
    origin: Url,
    request_timeout: Duration,
}

impl GithubProvider {
    pub fn new(
        id: String,
        organizations: Vec<String>,
        token: Secret,
    ) -> Result<Self, ProviderError> {
        if id.trim().is_empty()
            || id.len() > 128
            || organizations.is_empty()
            || organizations.len() > 100
            || organizations
                .iter()
                .any(|o| !safe_segment(o) || o.len() > 100)
            || token.expose().is_empty()
        {
            return Err(error("configuration"));
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("permesh/0.1")
            .build()
            .map_err(|_| error("configuration"))?;
        let mut organizations = organizations;
        organizations.sort_by_key(|o| o.to_ascii_lowercase());
        organizations.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        Ok(Self {
            id,
            organizations,
            token,
            client,
            origin: Url::parse("https://api.github.com").map_err(|_| error("configuration"))?,
            request_timeout: Duration::from_secs(15),
        })
    }
}

fn error(code: &str) -> ProviderError {
    ProviderError::new(
        code,
        match code {
            "configuration" => "Invalid GitHub provider configuration or HTTP client settings.",
            "unauthorized" => "GitHub rejected authentication; check the configured credential.",
            "forbidden" => {
                "GitHub denied access; check token permissions and organization authorization."
            }
            "not_found" => "GitHub resource was not visible; check scope and organization access.",
            "rate_limit" => {
                "GitHub rate limit prevented collection within the retry budget; retry later."
            }
            "timeout" => "GitHub request exceeded its deadline.",
            "transport" => "GitHub request could not be completed.",
            "limit" => {
                "GitHub collection reached a response, page, record, or request safety limit."
            }
            "pagination" => "GitHub returned invalid or unsafe pagination metadata.",
            "malformed" => "GitHub returned an invalid response or record.",
            "membership" => {
                "Active membership in a configured GitHub organization could not be verified."
            }
            "discovery" => {
                "No configured GitHub organization could be collected; check credentials and access."
            }
            "conflict" => {
                "Conflicting observations for the same native entity or grant were excluded with their dependent claims."
            }
            "clock" => "Observation time could not be formatted.",
            _ => "GitHub returned an unsuccessful response.",
        },
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "tests.rs"]
mod test_suite;

/// Native external-provider protocol entry point.
pub mod protocol;
