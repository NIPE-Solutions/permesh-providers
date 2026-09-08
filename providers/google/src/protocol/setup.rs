// SPDX-License-Identifier: MIT
use permesh_provider_sdk::setup::{Choice, Condition, Input, SetupField, SetupSpec, SetupStep};
use serde_json::json;
fn field(key: &str, label: &str, help: &str, input: Input, mode: Option<&str>) -> SetupField {
    SetupField {
        key: key.into(),
        label: label.into(),
        help: help.into(),
        required: true,
        default: None,
        when: mode.map(|v| Condition {
            field: "auth_mode".into(),
            equals: json!(v),
        }),
        input,
    }
}
pub(super) fn spec() -> SetupSpec {
    let mut mode = field(
        "auth_mode",
        "Authentication",
        "Use an existing access token or exchange an OAuth refresh token once per operation.",
        Input::Choice {
            options: vec![
                Choice {
                    value: "access_token".into(),
                    label: "Access token".into(),
                },
                Choice {
                    value: "refresh_token".into(),
                    label: "Refresh token".into(),
                },
            ],
        },
        None,
    );
    mode.default = Some(json!("access_token"));
    SetupSpec {schema_version:1,title:"Google Workspace Directory".into(),description:"Read users from one explicit customer using admin.directory.user.readonly. Primary email is directory-attested; identity kind remains unknown.".into(),
        steps:vec![SetupStep{id:"connection".into(),title:"Directory connection".into(),description:"OAuth credentials need administrator permission to read users for this customer. Store secrets through named references.".into(),when:None,fields:vec![
            field("customer_id","Customer ID","Use the stable Google Workspace customer ID starting with C; my_customer is not accepted.",Input::Text{min_length:2,max_length:128},None),
            mode,
            field("token","Access token reference","Use env://NAME or keychain://INSTANCE/token; never enter the token itself.",Input::Credential,Some("access_token")),
            field("client_id","OAuth client ID","Nonsecret ID of the OAuth client that issued the refresh token.",Input::Text{min_length:1,max_length:1024},Some("refresh_token")),
            field("refresh_token","Refresh token reference","Use env://NAME or keychain://INSTANCE/refresh_token. Authorize only admin.directory.user.readonly using offline access.",Input::Credential,Some("refresh_token")),
            field("client_secret","Client secret reference","Use env://NAME or keychain://INSTANCE/client_secret for the issuing OAuth client.",Input::Credential,Some("refresh_token")),
        ]}]}
}
