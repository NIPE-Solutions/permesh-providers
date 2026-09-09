// SPDX-License-Identifier: MIT
use super::{Configuration, IdentityCenterProvider};
use permesh_native_runtime::{Adapter, ProtocolFailure};
use permesh_provider_sdk::{
    Metadata,
    setup::{Input, SetupField, SetupSpec, SetupStep},
};
use serde::Deserialize;
use zeroize::Zeroizing;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    access_key_id: Zeroizing<String>,
    secret_access_key: Zeroizing<String>,
    session_token: Zeroizing<String>,
}
pub(crate) struct IdentityCenter;
impl Adapter for IdentityCenter {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn supports_network() -> bool {
        true
    }
    fn metadata() -> Metadata {
        super::provider_metadata()
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
        SetupSpec{schema_version:1,title:"AWS Identity Center inventory".into(),description:"Read a selected instance/store and explicit account allowlist using approved temporary AWS credentials.".into(),steps:vec![SetupStep{id:"connection".into(),title:"Scope and explicit temporary credentials".into(),description:"STS caller and ListInstances must prove the approved context before enumeration. No ambient profile, SSO, metadata or credential process is loaded.".into(),when:None,fields:vec![
 field("account_id","Caller account ID","Expected twelve-digit STS caller account.",true,Input::Text{min_length:12,max_length:12}),
 field("region","Identity Center region","Commercial AWS region containing the selected instance.",true,Input::Text{min_length:9,max_length:32}),
 field("instance_arn","Instance ARN","Exact approved arn:aws:sso:::instance/... ARN.",true,Input::Text{min_length:10,max_length:1224}),
 field("identity_store_id","Identity store ID","Exact store paired with the selected instance by ListInstances.",true,Input::Text{min_length:12,max_length:36}),
 field("accounts","Assignment account allowlist","One to 100 explicit twelve-digit account IDs; never expanded by Organizations.",true,Input::StringList{min_items:1,max_items:100}),
 field("caller_role","Caller role name","Optional exact STS assumed-role name; required when the host uses an approved profile source.",false,Input::Text{min_length:1,max_length:64}),
 SetupField{key:"include_organizations".into(),label:"Read organization account names".into(),help:"Optional organizations:ListAccounts enumeration; only allowlisted names are retained and assignment scope never expands.".into(),required:true,default:Some(serde_json::Value::Bool(false)),when:None,input:Input::Boolean},
 field("access_key_id","Temporary access key reference","env://NAME or keychain://INSTANCE/access_key_id.",true,Input::Credential),
 field("secret_access_key","Secret key reference","env://NAME or keychain://INSTANCE/secret_access_key.",true,Input::Credential),
 field("session_token","Session token reference","Required temporary session token; refresh externally or through a separately supported explicit host source.",true,Input::Credential),
 ]}]}
    }
    fn validate(c: &Configuration, k: &Credentials) -> bool {
        c.valid()
            && crate::auth::valid_values(
                &k.access_key_id,
                &k.secret_access_key,
                Some(&k.session_token),
            )
    }
    fn error_code(code: &str) -> &'static str {
        match code {
            "configuration" => "protocol_error",
            "scope" | "unauthorized" => "authentication",
            "forbidden" => "permission_denied",
            "rate_limit" => "rate_limited",
            "timeout" | "limit" | "transport" => "unavailable",
            _ => "internal",
        }
    }
    fn limitations(values: &[String]) -> Vec<&'static str> {
        let mut out = vec![];
        for value in values {
            let code = if super::VISIBILITY.contains(&value.as_str()) {
                "visibility_limited"
            } else if value.contains("forbidden") || value.contains("unauthorized") {
                "permission_denied"
            } else if value.contains("rate_limit") {
                "rate_limited"
            } else if value.contains("limit") {
                "page_limit"
            } else {
                "unknown"
            };
            if !out.contains(&code) {
                out.push(code);
            }
        }
        out
    }
}
pub async fn run<R: tokio::io::AsyncRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(
    reader: R,
    writer: W,
) -> Result<(), ProtocolFailure> {
    permesh_native_runtime::serve_with_network::<IdentityCenter, _, _, _, _, _>(
        reader,
        writer,
        |id, c, k, network| async move {
            let credentials = aws_credential_types::Credentials::new(
                k.access_key_id.as_str(),
                k.secret_access_key.as_str(),
                Some(k.session_token.to_string()),
                None,
                "PermeshExplicit",
            );
            IdentityCenterProvider::new(id, c, credentials, network.as_ref())
        },
    )
    .await
}
