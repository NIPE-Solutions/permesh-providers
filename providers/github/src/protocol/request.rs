// SPDX-License-Identifier: MIT OR Apache-2.0
use serde::{Deserialize, Deserializer};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Configuration {
    pub organizations: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Credentials {
    pub token: Zeroizing<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest {
    protocol: u32,
    id: String,
    method: String,
    #[serde(default, deserialize_with = "present")]
    instance: Option<String>,
    #[serde(default, deserialize_with = "present")]
    configuration: Option<Configuration>,
    #[serde(default, deserialize_with = "present")]
    credentials: Option<Credentials>,
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
pub(super) struct Request {
    pub protocol: u32,
    pub command: Command,
}
pub(super) enum Command {
    Handshake {
        instance: String,
    },
    Operation {
        check: bool,
        configuration: Configuration,
        credentials: Credentials,
    },
    Describe,
    Cancel,
}
pub(super) fn valid_instance(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}
pub(super) fn parse(bytes: &[u8]) -> Result<Request, ()> {
    if bytes.len() > super::MAX_FRAME {
        return Err(());
    }
    let value: WireRequest = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !matches!(value.protocol, 2 | 3) || value.id != value.method {
        return Err(());
    }
    let command = match value.method.as_str() {
        "handshake" if value.configuration.is_none() && value.credentials.is_none() => {
            let instance = value.instance.ok_or(())?;
            if !valid_instance(&instance) {
                return Err(());
            }
            Command::Handshake { instance }
        }
        "describe"
            if value.protocol == 3
                && value.instance.is_none()
                && value.configuration.is_none()
                && value.credentials.is_none() =>
        {
            Command::Describe
        }
        "cancel"
            if value.instance.is_none()
                && value.configuration.is_none()
                && value.credentials.is_none() =>
        {
            Command::Cancel
        }
        "check" | "discover" if value.protocol == 2 && value.instance.is_none() => {
            let configuration = value.configuration.ok_or(())?;
            let credentials = value.credentials.ok_or(())?;
            if credentials.token.is_empty() || credentials.token.len() > 16 * 1024 {
                return Err(());
            }
            Command::Operation {
                check: value.method == "check",
                configuration,
                credentials,
            }
        }
        _ => return Err(()),
    };
    Ok(Request {
        protocol: value.protocol,
        command,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn strict_request_schemas_reject_duplicates_unknowns_nulls_and_wrong_method_fields() {
        let handshake =
            br#"{"protocol":2,"id":"handshake","method":"handshake","instance":"github-main"}"#;
        assert!(parse(handshake).is_ok());
        for bytes in [
            &br#"{"protocol":2,"protocol":2,"id":"handshake","method":"handshake","instance":"github-main"}"#[..],
            &br#"{"protocol":2,"id":"handshake","method":"handshake","instance":"github-main","unknown":"SECRET"}"#[..],
            &br#"{"protocol":2,"id":"handshake","method":"handshake","instance":"github-main","configuration":null}"#[..],
            &br#"{"protocol":2,"id":"discover","method":"discover","configuration":{"organizations":["acme"],"endpoint":"SECRET"},"credentials":{"token":"SECRET"}}"#[..],
            &br#"{"protocol":2,"id":"check","method":"check","configuration":{"organizations":["acme"]},"credentials":{"token":"SECRET","token":"SECOND"}}"#[..],
            &br#"{"protocol":2,"id":"check","method":"check","configuration":{"organizations":["acme"]},"credentials":{"token":"SECRET","other":"SECOND"}}"#[..],
            &br#"{"protocol":3,"id":"describe","method":"describe","credentials":{"token":"SECRET"}}"#[..],
            &br#"{"protocol":3,"id":"discover","method":"discover","configuration":{"organizations":["acme"]},"credentials":{"token":"SECRET"}}"#[..],
            &br#"{"protocol":1,"id":"handshake","method":"handshake","instance":"github-main"}"#[..],
        ] {assert!(parse(bytes).is_err());}
    }
}
