// SPDX-License-Identifier: MIT
//! Bounded Microsoft Graph directory observations for one proved public-cloud tenant.
mod client;
mod discovery;
pub mod protocol;
mod records;
use permesh_core::Snapshot;
use permesh_provider_sdk::{Capability, Health, Metadata, Provider, ProviderError, ProviderFuture};
use permesh_secrets::Secret;
use reqwest::{Client, Url};
use std::time::Duration;
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_PAGES: usize = 100;
const MAX_ROWS: usize = 50_000;
const MAX_REQUESTS: usize = 1_000;
const VISIBILITY: &[&str] = &[
    "Entra observations are limited to the authenticated public-cloud tenant and token visibility; Graph listing is not transactional.",
    "Users are directory accounts, not proved humans or employees. Guest means external directory affiliation; Member does not prove employment. UPN and mail are not verified email evidence.",
    "Direct user and nested-group memberships are observed, not transitive or effective authorization. Graph v1.0 service-principal group memberships are excluded because the API has a documented omission.",
    "Devices, organizational contacts, invitations, tokens, sessions, app-role assignments, directory roles, Azure RBAC, PIM and Conditional Access are not collected.",
    "Canonical tenant/object identities require explicit host authority and stable account mappings; matching labels or IDs do not implicitly establish host identity bindings.",
];
pub struct EntraProvider {
    id: String,
    tenant_id: String,
    include_service_principals: bool,
    token: Secret,
    client: Client,
    origin: Url,
    request_timeout: Duration,
    operation_timeout: Duration,
}
impl EntraProvider {
    pub fn new(
        id: String,
        tenant_id: String,
        include_service_principals: bool,
        token: Secret,
    ) -> Result<Self, ProviderError> {
        Self::new_with_network(id, tenant_id, include_service_principals, token, None)
    }
    pub fn new_with_network(
        id: String,
        tenant_id: String,
        include_service_principals: bool,
        token: Secret,
        network: Option<&permesh_provider_sdk::network::NetworkContext>,
    ) -> Result<Self, ProviderError> {
        if !valid_instance(&id) || !records::uuid(&tenant_id) || !valid_token(token.expose()) {
            return Err(error("configuration"));
        }
        let client = permesh_native_runtime::network::client_builder(network)?
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .user_agent("permesh-entra/0.2")
            .build()
            .map_err(|_| error("configuration"))?;
        Ok(Self {
            id,
            tenant_id: tenant_id.to_ascii_lowercase(),
            include_service_principals,
            token,
            client,
            origin: Url::parse("https://graph.microsoft.com/v1.0/")
                .map_err(|_| error("configuration"))?,
            request_timeout: Duration::from_secs(10),
            operation_timeout: Duration::from_secs(45),
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
fn valid_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 16 * 1024 && token.bytes().all(|b| b.is_ascii_graphic())
}
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "entra".into(),
        capabilities: vec![
            Capability::Identities,
            Capability::Accounts,
            Capability::Groups,
            Capability::Memberships,
        ],
    }
}
impl Provider for EntraProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            tokio::time::timeout(self.operation_timeout,async{self.prove_tenant(&mut client::Budget::default()).await?;Ok(Health{message:"Entra tenant identity verified through Microsoft Graph organization; directory discovery permissions are not established by this check.".into(),limitations:self.limitations()})}).await.map_err(|_|error("timeout"))?
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
impl EntraProvider {
    fn limitations(&self) -> Vec<String> {
        let mut values: Vec<_> = VISIBILITY.iter().map(|s| (*s).into()).collect();
        if !self.include_service_principals {
            values.push("Entra service-principal inventory is disabled by configuration.".into());
        }
        values
    }
}
fn error(code: &str) -> ProviderError {
    ProviderError::new(
        code,
        match code {
            "configuration" => "Invalid Entra provider configuration or credential.",
            "scope" => "Microsoft Graph tenant proof did not match the configured tenant.",
            "unauthorized" => "Microsoft Graph rejected the credential.",
            "forbidden" => {
                "Microsoft Graph denied the requested directory operation; review the documented read permissions."
            }
            "rate_limit" => "Microsoft Graph throttling exceeded the bounded retry policy.",
            "timeout" => "Entra collection exceeded its deadline.",
            "limit" => "Entra collection exceeded a response, page, record or request budget.",
            "pagination" => "Microsoft Graph returned unsupported or inconsistent pagination.",
            "malformed" => {
                "Microsoft Graph returned malformed or conflicting directory observations."
            }
            _ => "Microsoft Graph request could not be completed.",
        },
    )
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod http_tests;
