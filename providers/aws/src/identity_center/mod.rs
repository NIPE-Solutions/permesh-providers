// SPDX-License-Identifier: MIT
//! Explicitly scoped Identity Center assignment inventory, separate from IAM.
mod discovery;
pub mod protocol;
mod records;
mod transport;
use aws_credential_types::Credentials;
use aws_sdk_ssoadmin::config::{
    BehaviorVersion, Region, retry::RetryConfig, timeout::TimeoutConfig,
};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use permesh_core::Snapshot;
use permesh_provider_sdk::{Capability, Health, Metadata, Provider, ProviderError, ProviderFuture};
use serde::Deserialize;
use std::{collections::BTreeSet, time::Duration};
const MAX_PAGES: usize = 100;
const MAX_ROWS: usize = 20_000;
const VISIBILITY: &[&str] = &[
    "Identity Center observations cover only the approved instance, identity store, region and account allowlist; collection is not transactional.",
    "Assignments are enumerated only through permission sets reported provisioned to each approved account; unprovisioned or failed-provisioning assignments are not independently enumerated.",
    "Permission-set assignments are observed configuration with unknown effective privilege. Policies, boundaries, SCPs/RCPs, resource policies, sessions, conditions and denies are not evaluated.",
    "Directory users are accounts of unknown principal kind and affiliation; labels and email fields do not prove human identity, employment or verified email. No canonical identity authority is emitted.",
    "Application assignments, provisioning state, invitations, tokens, sessions and organization hierarchy are not collected. Organizations discovery only decorates approved account resources and never expands assignment scope.",
];
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub account_id: String,
    pub region: String,
    pub instance_arn: String,
    pub identity_store_id: String,
    pub accounts: Vec<String>,
    #[serde(default)]
    pub include_organizations: bool,
    #[serde(default)]
    pub caller_role: Option<String>,
}
impl Configuration {
    pub fn valid(&self) -> bool {
        crate::valid_account(&self.account_id)
            && crate::valid_region(&self.region)
            && instance(&self.instance_arn)
            && native_store(&self.identity_store_id)
            && !self.accounts.is_empty()
            && self.accounts.len() <= 100
            && self.accounts.iter().all(|s| crate::valid_account(s))
            && self.accounts.iter().collect::<BTreeSet<_>>().len() == self.accounts.len()
            && self.caller_role.as_deref().is_none_or(crate::valid_role)
    }
}
fn instance(value: &str) -> bool {
    value
        .strip_prefix("arn:aws:sso:::instance/")
        .is_some_and(|s| {
            s.strip_prefix("ssoins-")
                .or_else(|| s.strip_prefix("ins-"))
                .is_some_and(|s| {
                    s.len() == 16
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
                })
        })
}
fn native_store(s: &str) -> bool {
    s.strip_prefix("d-")
        .is_some_and(|s| s.len() == 10 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        || records::uuid(s)
}
pub struct IdentityCenterProvider {
    id: String,
    config: Configuration,
    credentials: Credentials,
    sts: aws_sdk_sts::Client,
    sso: aws_sdk_ssoadmin::Client,
    store: aws_sdk_identitystore::Client,
    organizations: aws_sdk_organizations::Client,
}
impl IdentityCenterProvider {
    pub fn new(
        id: String,
        c: Configuration,
        k: Credentials,
        network: Option<&permesh_provider_sdk::network::NetworkContext>,
    ) -> Result<Self, ProviderError> {
        let endpoints = [
            format!("https://sts.{}.amazonaws.com", c.region),
            format!("https://sso.{}.amazonaws.com", c.region),
            format!("https://identitystore.{}.amazonaws.com", c.region),
            "https://organizations.us-east-1.amazonaws.com".into(),
        ];
        Self::build(id, c, k, endpoints, network)
    }
    fn build(
        id: String,
        c: Configuration,
        k: Credentials,
        endpoints: [String; 4],
        network: Option<&permesh_provider_sdk::network::NetworkContext>,
    ) -> Result<Self, ProviderError> {
        if !crate::valid_instance(&id)
            || !c.valid()
            || !crate::auth::valid_credentials(&k)
            || k.session_token().is_none()
            || [
                &id,
                &c.account_id,
                &c.region,
                &c.instance_arn,
                &c.identity_store_id,
            ]
            .iter()
            .any(|s| crate::reflects(&k, s))
            || c.accounts.iter().any(|s| crate::reflects(&k, s))
            || c.caller_role
                .as_deref()
                .is_some_and(|s| crate::reflects(&k, s))
        {
            return Err(error("configuration"));
        }
        let http = transport::client(endpoints.clone(), k.clone(), network)?;
        let retry = RetryConfig::standard().with_max_attempts(3);
        let timeout = TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(20))
            .operation_attempt_timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(10))
            .build();
        macro_rules! client {
            ($sdk:ident,$index:expr,$region:expr) => {
                $sdk::Client::from_conf(
                    $sdk::Config::builder()
                        .behavior_version(BehaviorVersion::latest())
                        .region(Region::new($region))
                        .credentials_provider(k.clone())
                        .http_client(http.clone())
                        .endpoint_url(endpoints[$index].clone())
                        .retry_config(retry.clone())
                        .timeout_config(timeout.clone())
                        .build(),
                )
            };
        }
        Ok(Self {
            id,
            sts: client!(aws_sdk_sts, 0, c.region.clone()),
            sso: client!(aws_sdk_ssoadmin, 1, c.region.clone()),
            store: client!(aws_sdk_identitystore, 2, c.region.clone()),
            organizations: client!(aws_sdk_organizations, 3, "us-east-1"),
            config: c,
            credentials: k,
        })
    }
    async fn prove(&self) -> Result<(), ProviderError> {
        let caller = self.sts.get_caller_identity().send().await.map_err(|e| {
            error(crate::service_code(
                e.as_service_error().and_then(|e| e.code()),
            ))
        })?;
        if caller.account() != Some(self.config.account_id.as_str())
            || caller.arn().is_none_or(|arn| {
                !crate::caller_matches(
                    arn,
                    &self.config.account_id,
                    self.config.caller_role.as_deref(),
                )
            })
        {
            return Err(error("scope"));
        }
        let mut pager = discovery::Pager::default();
        let mut found = false;
        loop {
            let v = self
                .sso
                .list_instances()
                .set_next_token(pager.token.clone())
                .max_results(100)
                .send()
                .await
                .map_err(|e| {
                    error(crate::service_code(
                        e.as_service_error().and_then(|e| e.code()),
                    ))
                })?;
            for instance in v.instances() {
                if instance.instance_arn() == Some(self.config.instance_arn.as_str()) {
                    if found
                        || instance.identity_store_id()
                            != Some(self.config.identity_store_id.as_str())
                    {
                        return Err(error("scope"));
                    }
                    found = true;
                }
            }
            if !pager.advance(v.next_token())? {
                break;
            }
        }
        if !found {
            return Err(error("scope"));
        }
        Ok(())
    }
}
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "aws-identity-center".into(),
        capabilities: vec![
            Capability::Accounts,
            Capability::Groups,
            Capability::Memberships,
            Capability::Resources,
            Capability::Grants,
        ],
    }
}
impl Provider for IdentityCenterProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            tokio::time::timeout(Duration::from_secs(50),async{self.prove().await?;Ok(Health{message:"AWS caller account/role and Identity Center instance/store binding verified; discovery permissions are not established by this check.".into(),limitations:VISIBILITY.iter().map(|s|(*s).into()).collect()})}).await.map_err(|_|error("timeout"))?
        })
    }
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        Box::pin(async {
            tokio::time::timeout(Duration::from_secs(50), self.collect())
                .await
                .map_err(|_| error("timeout"))?
        })
    }
}
fn error(code: &str) -> ProviderError {
    ProviderError::new(
        code,
        match code {
            "configuration" => {
                "Invalid Identity Center configuration or explicit temporary credentials."
            }
            "scope" => {
                "AWS caller account/role or Identity Center instance/store binding did not match approved scope."
            }
            "forbidden" => {
                "AWS denied an Identity Center inventory read; review the documented read permissions."
            }
            "unauthorized" => {
                "AWS rejected the configured temporary credentials; refresh the approved source."
            }
            "rate_limit" => "AWS throttling exceeded the bounded retry policy.",
            "timeout" => "Identity Center collection exceeded its deadline.",
            "pagination" => "AWS returned inconsistent inventory pagination.",
            "limit" => "Identity Center inventory exceeded a bounded request or record limit.",
            "malformed" => "AWS returned malformed or conflicting inventory metadata.",
            _ => "AWS Identity Center request could not be completed.",
        },
    )
}
#[cfg(test)]
mod tests;
