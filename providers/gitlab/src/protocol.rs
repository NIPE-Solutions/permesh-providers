// SPDX-License-Identifier: MIT
//! Thin provider-specific adapter for the shared native subprocess runtime.
use crate::GitlabProvider;
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
    pub(crate) origin: String,
    #[serde(default)]
    pub(crate) group_ids: Vec<String>,
    #[serde(default)]
    pub(crate) project_ids: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credentials {
    pub(crate) token: Zeroizing<String>,
}
pub(crate) struct Gitlab;
impl Adapter for Gitlab {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn supports_network() -> bool {
        true
    }
    fn metadata() -> Metadata {
        crate::provider_metadata()
    }
    fn setup() -> SetupSpec {
        SetupSpec {
            schema_version: 1,
            title: "GitLab scoped membership".into(),
            description: "Observe direct and collapsed effective group/project membership; no complete authorization evaluation.".into(),
            steps: vec![SetupStep {
                id: "connection".into(),
                title: "GitLab connection".into(),
                description: "Use a personal access token with read_api and visibility into the explicitly selected numeric groups/projects.".into(),
                when: None,
                fields: vec![
                    SetupField {key:"origin".into(),label:"Approved HTTPS origin".into(),help:"GitLab.com or explicit self-managed HTTPS origin, without path/query/credentials.".into(),required:true,default:Some("https://gitlab.com".into()),when:None,input:Input::Text{min_length:9,max_length:512}},
                    SetupField {key:"group_ids".into(),label:"Numeric group IDs".into(),help:"Directly owned projects are included; no subgroup traversal or shared-project expansion.".into(),required:false,default:Some(serde_json::json!([])),when:None,input:Input::StringList{min_items:0,max_items:50}},
                    SetupField {key:"project_ids".into(),label:"Numeric project IDs".into(),help:"Additional explicit projects; select at least one group or project in total.".into(),required:false,default:Some(serde_json::json!([])),when:None,input:Input::StringList{min_items:0,max_items:50}},
                    SetupField {
                        key: "token".into(), label: "API token reference".into(),
                        help: "Use env://NAME or keychain://INSTANCE/token. Supply a GitLab PAT with read_api through a secret reference.".into(),
                        required: true, default: None, when: None,
                        input: Input::Credential,
                    },
                ],
            }],
        }
    }

    fn validate(configuration: &Configuration, credentials: &Credentials) -> bool {
        crate::valid_configuration(
            &configuration.origin,
            &configuration.group_ids,
            &configuration.project_ids,
        ) && !credentials.token.is_empty()
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
                    "GitLab partial collection: forbidden" => "permission_denied",
                    "GitLab partial collection: unauthorized" => "permission_denied",
                    "GitLab partial collection: rate_limit" => "rate_limited",
                    "GitLab partial collection: limit" => "page_limit",
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
    permesh_native_runtime::serve_with_network::<Gitlab, _, _, _, _, _>(
        reader,
        writer,
        |id, configuration, mut credentials, network| async move {
            GitlabProvider::new_with_network(
                id,
                configuration.origin,
                configuration.group_ids,
                configuration.project_ids,
                Secret::new(std::mem::take(&mut *credentials.token)),
                network.as_ref(),
            )
        },
    )
    .await
}
