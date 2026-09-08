// SPDX-License-Identifier: MIT
use permesh_provider_sdk::{browser_auth::BrowserAuthSpec, setup::Condition};

pub(super) fn spec() -> BrowserAuthSpec {
    BrowserAuthSpec {
        schema_version: 1,
        authorization_endpoint: "https://accounts.google.com/o/oauth2/v2/auth".into(),
        token_endpoint: "https://oauth2.googleapis.com/token".into(),
        scopes: vec!["https://www.googleapis.com/auth/admin.directory.user.readonly".into()],
        client_id_field: "client_id".into(),
        client_secret_slot: Some("client_secret".into()),
        refresh_token_slot: "refresh_token".into(),
        when: Some(Condition {
            field: "auth_mode".into(),
            equals: serde_json::Value::String("refresh_token".into()),
        }),
        authorization_parameters: [
            ("access_type".into(), "offline".into()),
            ("prompt".into(), "consent".into()),
        ]
        .into(),
    }
}
