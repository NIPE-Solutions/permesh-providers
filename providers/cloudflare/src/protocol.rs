// SPDX-License-Identifier: MIT OR Apache-2.0
//! Thin provider-specific adapter for the shared native subprocess runtime.
use crate::{CloudflareProvider, records};
use permesh_native_runtime::Adapter;
pub use permesh_native_runtime::ProtocolFailure;
use permesh_provider_sdk::{
    Metadata,
    setup::{Input, SetupField, SetupSpec, SetupStep},
};
use permesh_secrets::Secret;
use serde::Deserialize;
use tokio::io::{AsyncRead, AsyncWrite};
use zeroize::Zeroizing;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Configuration {
    pub(crate) account_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    pub(crate) token: Zeroizing<String>,
}
pub(crate) struct Cloudflare;
impl Adapter for Cloudflare {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn metadata() -> Metadata {
        crate::provider_metadata()
    }
    fn setup() -> SetupSpec {
        SetupSpec{schema_version:1,title:"Cloudflare account".into(),description:"Read account members, IAM groups, policy assignments and visible zones. Assignments retain unknown effective privilege.".into(),steps:vec![SetupStep{id:"connection".into(),title:"Account connection".into(),description:"Use Account Settings Read and Zone Read scoped to the intended account and zones.".into(),when:None,fields:vec![
 SetupField{key:"account_id".into(),label:"Account ID".into(),help:"Cloudflare account ID: 32 lowercase hexadecimal characters.".into(),required:true,default:None,when:None,input:Input::Text{min_length:32,max_length:32}},
 SetupField{key:"token".into(),label:"API token reference".into(),help:"Use env://NAME or keychain://INSTANCE/token. Supply an account-owned or user API token through a secret reference.".into(),required:true,default:None,when:None,input:Input::Credential},
 ]}]}
    }
    fn validate(configuration: &Configuration, credentials: &Credentials) -> bool {
        records::native_id(&configuration.account_id)
            && !credentials.token.is_empty()
            && credentials.token.len() <= 16 * 1024
            && credentials.token.bytes().all(|b| b.is_ascii_graphic())
    }
    fn error_code(code: &str) -> &'static str {
        match code {
            "configuration" => "protocol_error",
            "unauthorized" => "authentication",
            "forbidden" => "permission_denied",
            "rate_limit" => "rate_limited",
            "timeout" | "transport" | "limit" => "unavailable",
            _ => "internal",
        }
    }
    fn limitations(values: &[String]) -> Vec<&'static str> {
        let mut result = Vec::new();
        for value in values {
            let code = if crate::VISIBILITY.contains(&value.as_str()) {
                "visibility_limited"
            } else {
                match value.as_str() {
                    "Cloudflare partial collection: forbidden." => "permission_denied",
                    "Cloudflare partial collection: unauthorized." => "permission_denied",
                    "Cloudflare partial collection: rate_limit." => "rate_limited",
                    "Cloudflare partial collection: limit." => "page_limit",
                    _ => "unknown",
                }
            };
            if !result.contains(&code) {
                result.push(code)
            }
        }
        result
    }
}
pub async fn run<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: R,
    writer: W,
) -> Result<(), ProtocolFailure> {
    permesh_native_runtime::serve::<Cloudflare, _, _, _, _, _>(
        reader,
        writer,
        |id, configuration, mut credentials| async move {
            CloudflareProvider::new(
                id,
                configuration.account_id,
                Secret::new(std::mem::take(&mut *credentials.token)),
            )
        },
    )
    .await
}
