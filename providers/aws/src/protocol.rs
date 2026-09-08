// SPDX-License-Identifier: MIT
//! AWS adapter for the shared native runtime. Credentials are explicit named references.
use permesh_native_runtime::{Adapter, ProtocolFailure};
use permesh_provider_sdk::{
    Metadata,
    setup::{Input, SetupField, SetupSpec, SetupStep},
};
use permesh_secrets::Secret;
use serde::Deserialize;
use zeroize::Zeroizing;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    account_id: String,
    region: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    access_key_id: Zeroizing<String>,
    secret_access_key: Zeroizing<String>,
    #[serde(default)]
    session_token: Option<Zeroizing<String>>,
}
struct Aws;
impl Adapter for Aws {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn metadata() -> Metadata {
        crate::provider_metadata()
    }
    fn setup() -> SetupSpec {
        let field = |key: &str, label: &str, help: &str, required, input| SetupField {
            key: key.into(),
            label: label.into(),
            help: help.into(),
            required,
            default: None,
            when: None,
            input,
        };
        SetupSpec{schema_version:1,title:"AWS IAM attachment inventory".into(),description:"Inspect one account using explicit AWS credential references. No ambient AWS profile, SSO, credential process or endpoint configuration is loaded.".into(),steps:vec![SetupStep{id:"connection".into(),title:"Account and credential references".into(),description:"Requires iam:GetAccountAuthorizationDetails. STS GetCallerIdentity verifies the account. Temporary credentials require all three references and must remain valid for the collection.".into(),when:None,fields:vec![
field("account_id","AWS account ID","Expected twelve-digit commercial AWS account ID.",true,Input::Text{min_length:12,max_length:12}),
field("region","STS region","Commercial AWS region such as eu-west-1. IAM uses its global commercial endpoint.",true,Input::Text{min_length:9,max_length:32}),
field("access_key_id","Access key ID reference","Use env://NAME or keychain://INSTANCE/access_key_id.",true,Input::Credential),
field("secret_access_key","Secret access key reference","Use env://NAME or keychain://INSTANCE/secret_access_key.",true,Input::Credential),
field("session_token","Session token reference","Required for temporary credentials. Use env://NAME or keychain://INSTANCE/session_token. Refresh externally before collection.",false,Input::Credential),
]}]}
    }
    fn validate(c: &Configuration, k: &Credentials) -> bool {
        crate::valid_account(&c.account_id)
            && crate::valid_region(&c.region)
            && crate::auth::valid_values(
                &k.access_key_id,
                &k.secret_access_key,
                k.session_token.as_deref().map(|v| v.as_str()),
            )
    }
    fn error_code(code: &str) -> &'static str {
        match code {
            "configuration" => "protocol_error",
            "unauthorized" | "account_mismatch" => "authentication",
            "forbidden" => "permission_denied",
            "rate_limit" => "rate_limited",
            "timeout" | "transport" | "limit" => "unavailable",
            _ => "internal",
        }
    }
    fn limitations(values: &[String]) -> Vec<&'static str> {
        let mut out = vec![];
        for v in values {
            let code = if crate::VISIBILITY.contains(&v.as_str()) {
                "visibility_limited"
            } else if v.contains("forbidden") || v.contains("unauthorized") {
                "permission_denied"
            } else if v.contains("rate_limit") {
                "rate_limited"
            } else if v.contains("limit") {
                "page_limit"
            } else {
                "unknown"
            };
            if !out.contains(&code) {
                out.push(code)
            }
        }
        out
    }
}
pub async fn run<R: tokio::io::AsyncRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    reader: R,
    writer: W,
) -> Result<(), ProtocolFailure> {
    permesh_native_runtime::serve::<Aws, _, _, _, _, _>(reader, writer, |id, c, mut k| async move {
        crate::AwsProvider::new(
            id,
            c.account_id,
            c.region,
            Secret::new(std::mem::take(&mut *k.access_key_id)),
            Secret::new(std::mem::take(&mut *k.secret_access_key)),
            k.session_token
                .as_mut()
                .map(|v| Secret::new(std::mem::take(&mut **v))),
        )
    })
    .await
}
