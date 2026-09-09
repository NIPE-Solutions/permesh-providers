// SPDX-License-Identifier: MIT
//! Native Google Directory provider protocol.
mod browser_auth;
mod setup;
use crate::{GoogleProvider, auth};
use permesh_native_runtime::Adapter;
pub use permesh_native_runtime::ProtocolFailure;
use permesh_provider_sdk::{Metadata, setup::SetupSpec};
use permesh_secrets::Secret;
use serde::{Deserialize, Deserializer};
use tokio::io::{AsyncRead, AsyncWrite};
use zeroize::Zeroizing;
#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum AuthMode {
    #[default]
    AccessToken,
    RefreshToken,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    customer_id: String,
    #[serde(default)]
    auth_mode: AuthMode,
    #[serde(default, deserialize_with = "present")]
    client_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    #[serde(default, deserialize_with = "present")]
    token: Option<Zeroizing<String>>,
    #[serde(default, deserialize_with = "present")]
    refresh_token: Option<Zeroizing<String>>,
    #[serde(default, deserialize_with = "present")]
    client_secret: Option<Zeroizing<String>>,
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
struct Google;
impl Adapter for Google {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn supports_network() -> bool {
        true
    }
    fn metadata() -> Metadata {
        crate::provider_metadata()
    }
    fn setup() -> SetupSpec {
        setup::spec()
    }
    fn browser_auth() -> Option<permesh_provider_sdk::browser_auth::BrowserAuthSpec> {
        Some(browser_auth::spec())
    }
    fn validate(config: &Configuration, credentials: &Credentials) -> bool {
        let customer = &config.customer_id;
        if !(2..=128).contains(&customer.len())
            || !customer.starts_with('C')
            || !customer.bytes().all(|c| c.is_ascii_alphanumeric())
        {
            return false;
        }
        let valid = |value: &Option<Zeroizing<String>>| {
            value.as_ref().is_some_and(|v| auth::valid_token(v))
        };
        match config.auth_mode {
            AuthMode::AccessToken => {
                config.client_id.is_none()
                    && valid(&credentials.token)
                    && credentials.refresh_token.is_none()
                    && credentials.client_secret.is_none()
            }
            AuthMode::RefreshToken => {
                config.client_id.as_ref().is_some_and(|v| {
                    !v.is_empty()
                        && v.len() <= 1024
                        && v.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
                }) && credentials.token.is_none()
                    && valid(&credentials.refresh_token)
                    && valid(&credentials.client_secret)
            }
        }
    }
    fn error_code(code: &str) -> &'static str {
        match code {
            "configuration" => "protocol_error",
            "unauthorized" => "authentication",
            "forbidden" => "permission_denied",
            "rate_limit" => "rate_limited",
            "transport" | "timeout" | "limit" => "unavailable",
            _ => "internal",
        }
    }
    fn limitations(values: &[String]) -> Vec<&'static str> {
        let mut result = Vec::new();
        for value in values {
            let code = if crate::VISIBILITY.contains(&value.as_str()) {
                "visibility_limited"
            } else if value == &crate::error("forbidden").message {
                "permission_denied"
            } else if value == &crate::error("rate_limit").message {
                "rate_limited"
            } else if value == &crate::error("limit").message {
                "page_limit"
            } else {
                "unknown"
            };
            if !result.contains(&code) {
                result.push(code);
            }
        }
        result
    }
}
fn secret(value: Option<Zeroizing<String>>) -> Result<Secret, permesh_provider_sdk::ProviderError> {
    value
        .map(|mut value| Secret::new(std::mem::take(&mut *value)))
        .ok_or_else(|| crate::error("configuration"))
}
pub async fn run<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: R,
    writer: W,
) -> Result<(), ProtocolFailure> {
    permesh_native_runtime::serve_with_network::<Google, _, _, _, _, _>(
        reader,
        writer,
        |id, config, credentials, network| async move {
            let token = match config.auth_mode {
                AuthMode::AccessToken => secret(credentials.token)?,
                AuthMode::RefreshToken => {
                    let refresh = secret(credentials.refresh_token)?;
                    let client_secret = secret(credentials.client_secret)?;
                    let client_id = config
                        .client_id
                        .ok_or_else(|| crate::error("configuration"))?;
                    auth::refresh(&client_id, &refresh, &client_secret, network.as_ref()).await?
                }
            };
            GoogleProvider::new_with_network(id, config.customer_id, token, network.as_ref())
        },
    )
    .await
}
#[cfg(test)]
mod tests;
