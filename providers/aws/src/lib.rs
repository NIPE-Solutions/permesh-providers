// SPDX-License-Identifier: MIT
//! AWS IAM policy attachment inventory. No effective IAM evaluation or ambient authentication.
mod auth;
pub mod identity_center;
pub mod protocol;
mod records;
mod transport;
use aws_credential_types::Credentials;
use aws_sdk_iam::config::{BehaviorVersion, Region, retry::RetryConfig, timeout::TimeoutConfig};
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use permesh_core::Snapshot;
use permesh_provider_sdk::{Capability, Health, Metadata, Provider, ProviderError, ProviderFuture};
use permesh_secrets::Secret;
use std::{collections::BTreeSet, time::Duration};
const MAX_PAGES: usize = 100;
const MAX_ROWS: usize = 20_000;
const VISIBILITY: &[&str] = &[
    "AWS IAM policy attachments are observed configuration evidence with unknown effective privilege; policies are not evaluated.",
    "Trust policies, boundaries, session policies, resource policies, SCPs/RCPs, conditions and explicit denies are not represented as effective grants. No role-assumption paths are inferred.",
    "One configured commercial AWS account is inspected. Identity Center assignments, organization discovery, service resources, root access and verified email are not enumerated.",
    "IAM collection is not transactional. Completed bounded collection does not prove complete effective access; attachment targets are policy evidence resources.",
];
pub struct AwsProvider {
    id: String,
    account: String,
    credentials: Credentials,
    caller_role: Option<String>,
    iam: aws_sdk_iam::Client,
    sts: aws_sdk_sts::Client,
}
impl AwsProvider {
    pub fn new(
        id: String,
        account: String,
        region: String,
        access_key_id: Secret,
        secret_access_key: Secret,
        session_token: Option<Secret>,
    ) -> Result<Self, ProviderError> {
        let credentials = Credentials::new(
            access_key_id.expose(),
            secret_access_key.expose(),
            session_token.as_ref().map(|s| s.expose().to_owned()),
            None,
            "Permesh",
        );
        Self::from_credentials(id, account, region, credentials)
    }
    fn from_credentials(
        id: String,
        account: String,
        region: String,
        credentials: Credentials,
    ) -> Result<Self, ProviderError> {
        let sts = format!("https://sts.{region}.amazonaws.com");
        Self::build(
            id,
            account,
            region,
            credentials,
            "https://iam.amazonaws.com".into(),
            sts,
        )
    }
    fn build(
        id: String,
        account: String,
        region: String,
        credentials: Credentials,
        iam_endpoint: String,
        sts_endpoint: String,
    ) -> Result<Self, ProviderError> {
        if !valid_instance(&id)
            || !valid_account(&account)
            || !valid_region(&region)
            || !auth::valid_credentials(&credentials)
        {
            return Err(error("configuration"));
        }
        if [id.as_str(), account.as_str()]
            .iter()
            .any(|v| reflects(&credentials, v))
        {
            return Err(error("configuration"));
        }
        let http = transport::client(iam_endpoint.clone(), sts_endpoint.clone())?;
        let retry = RetryConfig::standard().with_max_attempts(3);
        let timeout = TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(30))
            .operation_attempt_timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(15))
            .build();
        let iam = aws_sdk_iam::Client::from_conf(
            aws_sdk_iam::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new("us-east-1"))
                .credentials_provider(credentials.clone())
                .http_client(http.clone())
                .endpoint_url(iam_endpoint)
                .retry_config(retry.clone())
                .timeout_config(timeout.clone())
                .build(),
        );
        let sts = aws_sdk_sts::Client::from_conf(
            aws_sdk_sts::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new(region))
                .credentials_provider(credentials.clone())
                .http_client(http)
                .endpoint_url(sts_endpoint)
                .retry_config(retry)
                .timeout_config(timeout)
                .build(),
        );
        Ok(Self {
            id,
            account,
            credentials,
            caller_role: None,
            iam,
            sts,
        })
    }
    pub fn with_caller_role(mut self, role: Option<String>) -> Result<Self, ProviderError> {
        if role
            .as_deref()
            .is_some_and(|s| !valid_role(s) || reflects(&self.credentials, s))
        {
            return Err(error("configuration"));
        }
        self.caller_role = role;
        Ok(self)
    }
    async fn caller(&self) -> Result<(), ProviderError> {
        let output = self
            .sts
            .get_caller_identity()
            .send()
            .await
            .map_err(|e| error(service_code(e.as_service_error().and_then(|e| e.code()))))?;
        if output.account() != Some(self.account.as_str())
            || output
                .arn()
                .is_none_or(|arn| !caller_matches(arn, &self.account, self.caller_role.as_deref()))
        {
            return Err(error("account_mismatch"));
        }
        Ok(())
    }
    async fn page(&self,marker:Option<String>,size:i32)->Result<aws_sdk_iam::operation::get_account_authorization_details::GetAccountAuthorizationDetailsOutput,ProviderError>{
        use aws_sdk_iam::types::EntityType;
        self.iam
            .get_account_authorization_details()
            .set_marker(marker)
            .max_items(size)
            .set_filter(Some(vec![
                EntityType::User,
                EntityType::Role,
                EntityType::Group,
                EntityType::LocalManagedPolicy,
                EntityType::AwsManagedPolicy,
            ]))
            .send()
            .await
            .map_err(|e| error(service_code(e.as_service_error().and_then(|e| e.code()))))
    }
    async fn collect(&self) -> Result<Snapshot, ProviderError> {
        self.caller().await?;
        let mut collected = records::Collected::default();
        let mut marker = None;
        let mut markers = BTreeSet::new();
        for page in 0..MAX_PAGES {
            let output = match self.page(marker, 100).await {
                Ok(value) => value,
                Err(e) if page == 0 => return Err(e),
                Err(e) => {
                    collected.partial(&e.code);
                    break;
                }
            };
            let more = output.is_truncated();
            let next = output.marker().map(str::to_owned);
            collected.add(output)?;
            if !more {
                if next.as_ref().is_some_and(|s| !s.is_empty()) {
                    collected.partial("pagination")
                }
                break;
            }
            marker = match next {
                Some(value)
                    if !value.is_empty()
                        && value.len() <= 4096
                        && !value.chars().any(char::is_control)
                        && markers.insert(value.clone()) =>
                {
                    Some(value)
                }
                _ => {
                    collected.partial("pagination");
                    break;
                }
            };
            if page + 1 == MAX_PAGES {
                collected.partial("limit")
            }
        }
        collected.finish(self)
    }
}
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "aws".into(),
        capabilities: vec![
            Capability::Accounts,
            Capability::Resources,
            Capability::Groups,
            Capability::Memberships,
            Capability::Grants,
        ],
    }
}
impl Provider for AwsProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            tokio::time::timeout(Duration::from_secs(50),async{self.caller().await?;self.page(None,1).await?;Ok(Health{message:"AWS STS account identity and bounded IAM authorization-detail probes succeeded; effective access was not evaluated.".into(),limitations:VISIBILITY.iter().map(|s|(*s).into()).collect()})}).await.map_err(|_|error("timeout"))?
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
fn valid_instance(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.as_bytes()[0].is_ascii_alphabetic()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn valid_account(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| b.is_ascii_digit())
}
fn valid_region(s: &str) -> bool {
    s.len() <= 32
        && !s.starts_with("cn-")
        && !s.starts_with("us-gov-")
        && s.split('-').count() == 3
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && s.as_bytes().last().is_some_and(u8::is_ascii_digit)
}
fn reflects(c: &Credentials, s: &str) -> bool {
    s.contains(c.access_key_id())
        || s.contains(c.secret_access_key())
        || c.session_token().is_some_and(|token| s.contains(token))
}
fn service_code(code: Option<&str>) -> &'static str {
    match code {
        Some("AccessDenied" | "AccessDeniedException") => "forbidden",
        Some(
            "InvalidClientTokenId"
            | "ExpiredToken"
            | "SignatureDoesNotMatch"
            | "InvalidSignatureException",
        ) => "unauthorized",
        Some("Throttling" | "ThrottlingException" | "RequestLimitExceeded") => "rate_limit",
        _ => "transport",
    }
}
fn error(code: &str) -> ProviderError {
    ProviderError::new(
        code,
        match code {
            "configuration" => "Invalid AWS provider configuration or explicit credentials.",
            "account_mismatch" => "AWS caller account does not match the configured account.",
            "forbidden" => {
                "AWS denied iam:GetAccountAuthorizationDetails; check the read-only policy."
            }
            "unauthorized" => "AWS rejected the configured credential or session token.",
            "rate_limit" => "AWS throttling exhausted the bounded retry budget.",
            "timeout" => "AWS collection exceeded its deadline.",
            "limit" => "AWS collection exceeded a record or request limit.",
            "malformed" => "AWS returned incomplete, conflicting or unsupported metadata.",
            _ => "AWS request could not be completed.",
        },
    )
}
#[cfg(test)]
mod tests;

pub(crate) fn valid_role(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'=' | b',' | b'.' | b'@' | b'-')
        })
}
pub(crate) fn caller_matches(arn: &str, account: &str, role: Option<&str>) -> bool {
    if arn.len() > 2048
        || arn.chars().any(char::is_control)
        || !arn.starts_with("arn:aws:")
        || arn.split(':').nth(4) != Some(account)
    {
        return false;
    }
    role.is_none_or(|role| {
        arn.strip_prefix(&format!("arn:aws:sts::{account}:assumed-role/{role}/"))
            .is_some_and(|session| {
                !session.is_empty() && session.len() <= 64 && !session.contains('/')
            })
    })
}
