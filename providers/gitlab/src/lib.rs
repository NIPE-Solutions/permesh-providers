// SPDX-License-Identifier: MIT
//! Scoped GitLab membership/role observations; no effective authorization evaluation.
mod client;
mod discovery;
pub mod protocol;
mod records;
use permesh_core::Snapshot;
use permesh_provider_sdk::{Capability, Health, Metadata, Provider, ProviderError, ProviderFuture};
use permesh_secrets::Secret;
use reqwest::{Client, Url};
use std::{collections::BTreeSet, time::Duration};
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BODY: usize = 32 * 1024 * 1024;
const MAX_PAGES: usize = 100;
const MAX_ROWS: usize = 20_000;
const MAX_REQUESTS: usize = 2_000;
const VISIBILITY: &[&str] = &[
    "GitLab API/token permissions and edition limit visibility; observations are not transactional and absence does not prove no access.",
    "Only explicit groups, their directly owned projects, and explicit projects are collected; subgroup traversal and shared project expansion are excluded.",
    "Direct members are assignment evidence; members/all is observed effective membership with collapsed inheritance/sharing paths, not direct assignment or full authorization evaluation.",
    "No authoritative identities, verified email, invitations, token/session inventory, custom-role permission evaluation, ownership transfer, branch rules or application permissions are collected.",
];
pub struct GitlabProvider {
    id: String,
    scope: String,
    origin: Url,
    group_ids: Vec<u64>,
    project_ids: Vec<u64>,
    token: Secret,
    client: Client,
    request_timeout: Duration,
    operation_timeout: Duration,
}
fn origin(value: &str) -> Result<Url, ProviderError> {
    if value.len() > 512
        || !value.bytes().all(|b| b.is_ascii_graphic())
        || value.contains(['@', '?', '#', '\\', '%'])
    {
        return Err(error("configuration"));
    }
    let authority = value
        .strip_prefix("https://")
        .ok_or_else(|| error("configuration"))?;
    if authority
        .split_once('/')
        .is_some_and(|(_, path)| !path.is_empty())
    {
        return Err(error("configuration"));
    }
    let url = Url::parse(value).map_err(|_| error("configuration"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error("configuration"));
    }
    Ok(url)
}
fn ids(values: &[String]) -> Result<Vec<u64>, ProviderError> {
    let mut seen = BTreeSet::new();
    for value in values {
        let id = value
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0 && id.to_string() == *value)
            .ok_or_else(|| error("configuration"))?;
        if !seen.insert(id) {
            return Err(error("configuration"));
        }
    }
    Ok(seen.into_iter().collect())
}
fn valid_configuration(origin_value: &str, groups: &[String], projects: &[String]) -> bool {
    (1..=50).contains(&(groups.len() + projects.len()))
        && origin(origin_value).is_ok()
        && ids(groups).is_ok()
        && ids(projects).is_ok()
}
pub fn valid_instance(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.as_bytes()[0].is_ascii_alphabetic()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
impl GitlabProvider {
    pub fn new(
        id: String,
        origin_value: String,
        group_ids: Vec<String>,
        project_ids: Vec<String>,
        token: Secret,
    ) -> Result<Self, ProviderError> {
        Self::new_with_network(id, origin_value, group_ids, project_ids, token, None)
    }
    pub fn new_with_network(
        id: String,
        origin_value: String,
        group_ids: Vec<String>,
        project_ids: Vec<String>,
        token: Secret,
        network: Option<&permesh_provider_sdk::network::NetworkContext>,
    ) -> Result<Self, ProviderError> {
        if !valid_instance(&id)
            || !valid_configuration(&origin_value, &group_ids, &project_ids)
            || token.expose().is_empty()
            || token.expose().len() > 16 * 1024
            || !token.expose().bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(error("configuration"));
        }
        let origin = origin(&origin_value)?;
        let scope = format!(
            "gitlab:{}:{}",
            origin.host_str().ok_or_else(|| error("configuration"))?,
            origin
                .port_or_known_default()
                .ok_or_else(|| error("configuration"))?
        );
        let client = permesh_native_runtime::network::client_builder(network)?
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("permesh-gitlab/0.2.0")
            .build()
            .map_err(|_| error("configuration"))?;
        Ok(Self {
            id,
            scope,
            origin,
            group_ids: ids(&group_ids)?,
            project_ids: ids(&project_ids)?,
            token,
            client,
            request_timeout: Duration::from_secs(15),
            operation_timeout: Duration::from_secs(50),
        })
    }
}
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "gitlab".into(),
        capabilities: vec![
            Capability::Accounts,
            Capability::Resources,
            Capability::Groups,
            Capability::Memberships,
            Capability::Grants,
        ],
    }
}
impl Provider for GitlabProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            tokio::time::timeout(self.operation_timeout,async {
            let mut budget=client::Budget::default();
            let user=self.request("user",&[],&mut budget).await?.0;
            records::account(&self.id,&self.scope,&user)?;
            for id in &self.group_ids {let value=self.request(&format!("groups/{id}"),&[],&mut budget).await?.0;records::resource(&self.id,&self.scope,"group",*id,&value)?;}
            for id in &self.project_ids {let value=self.request(&format!("projects/{id}"),&[],&mut budget).await?.0;records::resource(&self.id,&self.scope,"project",*id,&value)?;}
            Ok(Health {message:"GitLab identity and explicit scope probes succeeded; full discovery visibility is not established.".into(),limitations:VISIBILITY.iter().map(|s|(*s).into()).collect()})
        }).await.map_err(|_|error("timeout"))?
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
            "configuration" => "Invalid GitLab origin, numeric scope or credential configuration.",
            "unauthorized" => "GitLab rejected the credential.",
            "forbidden" => {
                "GitLab denied access; review token read_api scope and resource visibility."
            }
            "scope" => "GitLab returned an unexpected resource scope.",
            "pagination" => {
                "GitLab pagination was inconsistent or outside the approved request scope."
            }
            "rate_limit" => "GitLab rate limit exceeds the bounded retry budget.",
            "timeout" => "GitLab collection exceeded its deadline.",
            "limit" => "GitLab response, page, record or request limit exceeded.",
            "malformed" => "GitLab returned malformed or contradictory metadata.",
            _ => "GitLab request could not be completed.",
        },
    )
}
#[cfg(test)]
mod tests;
