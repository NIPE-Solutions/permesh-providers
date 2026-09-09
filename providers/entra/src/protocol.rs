// SPDX-License-Identifier: MIT
//! Thin adapter for the existing negotiated discovery and legacy setup runtimes.
use crate::{EntraProvider, records};
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
    tenant_id: String,
    #[serde(default)]
    include_service_principals: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    token: Zeroizing<String>,
}
pub(crate) struct Entra;
impl Adapter for Entra {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn supports_network() -> bool {
        true
    }
    fn metadata() -> Metadata {
        crate::provider_metadata()
    }
    fn setup() -> SetupSpec {
        SetupSpec{schema_version:1,title:"Microsoft Entra directory".into(),description:"Observe directory objects and direct user/group memberships in one verified public-cloud tenant. No effective authorization or employment conclusions.".into(),steps:vec![SetupStep{id:"connection".into(),title:"Tenant connection".into(),description:"Use an explicitly named Graph token with Organization.Read.All, User.Read.All and GroupMember.Read.All. Optional service-principal inventory also requires Application.Read.All; administrator consent is required for application permissions.".into(),when:None,fields:vec![SetupField{key:"tenant_id".into(),label:"Tenant ID".into(),help:"Explicit tenant UUID. Graph organization must prove the same tenant before enumeration.".into(),required:true,default:None,when:None,input:Input::Text{min_length:36,max_length:36}},SetupField{key:"include_service_principals".into(),label:"Include service-principal objects".into(),help:"Requires Application.Read.All. Service-principal group memberships remain excluded due to the documented Graph v1 limitation.".into(),required:true,default:Some(serde_json::Value::Bool(false)),when:None,input:Input::Boolean},SetupField{key:"token".into(),label:"Graph token reference".into(),help:"Use env://NAME or keychain://INSTANCE/token. Supply a Graph access token through a secret reference; no automatic OAuth, ambient credentials or JWT tenant inference.".into(),required:true,default:None,when:None,input:Input::Credential}]}]}
    }
    fn validate(configuration: &Configuration, credentials: &Credentials) -> bool {
        records::uuid(&configuration.tenant_id) && crate::valid_token(&credentials.token)
    }
    fn error_code(code: &str) -> &'static str {
        match code {
            "configuration" => "protocol_error",
            "unauthorized" => "authentication",
            "forbidden" | "scope" => "permission_denied",
            "rate_limit" => "rate_limited",
            "timeout" | "transport" | "limit" => "unavailable",
            _ => "internal",
        }
    }
    fn limitations(values: &[String]) -> Vec<&'static str> {
        let mut result = vec![];
        for value in values {
            let code = match value.as_str() {
                "Entra partial collection: unauthorized."
                | "Entra partial collection: forbidden." => "permission_denied",
                "Entra partial collection: rate_limit." => "rate_limited",
                "Entra partial collection: limit." => "page_limit",
                s if crate::VISIBILITY.contains(&s)
                    || s == "Entra service-principal inventory is disabled by configuration." =>
                {
                    "visibility_limited"
                }
                _ => "unknown",
            };
            if !result.contains(&code) {
                result.push(code);
            }
        }
        result
    }
}
pub async fn run<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: R,
    writer: W,
) -> Result<(), ProtocolFailure> {
    permesh_native_runtime::serve_with_network::<Entra, _, _, _, _, _>(
        reader,
        writer,
        |id, configuration, mut credentials, network| async move {
            EntraProvider::new_with_network(
                id,
                configuration.tenant_id,
                configuration.include_service_principals,
                Secret::new(std::mem::take(&mut *credentials.token)),
                network.as_ref(),
            )
        },
    )
    .await
}
