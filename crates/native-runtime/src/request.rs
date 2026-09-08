// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::Adapter;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(
    deserialize = "A::Configuration: Deserialize<'de>, A::Credentials: Deserialize<'de>"
))]
struct WireRequest<A: Adapter> {
    protocol: u32,
    id: String,
    method: String,
    #[serde(default, deserialize_with = "present")]
    instance: Option<String>,
    #[serde(default, deserialize_with = "present")]
    configuration: Option<A::Configuration>,
    #[serde(default, deserialize_with = "present")]
    credentials: Option<A::Credentials>,
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
pub(super) struct Request<A: Adapter> {
    pub protocol: u32,
    pub command: Command<A>,
}
pub(super) enum Command<A: Adapter> {
    Handshake {
        instance: String,
    },
    Operation {
        check: bool,
        configuration: A::Configuration,
        credentials: A::Credentials,
    },
    Describe,
    DescribeAuth,
    Cancel,
}
pub(super) fn valid_instance(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}
pub(super) fn parse<A: Adapter>(bytes: &[u8]) -> Result<Request<A>, ()> {
    if bytes.len() > super::MAX_FRAME {
        return Err(());
    }
    let value: WireRequest<A> = serde_json::from_slice(bytes).map_err(|_| ())?;
    if !matches!(value.protocol, 2..=4) || value.id != value.method {
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
        "describe_auth"
            if value.protocol == 4
                && value.instance.is_none()
                && value.configuration.is_none()
                && value.credentials.is_none() =>
        {
            Command::DescribeAuth
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
            if !A::validate(&configuration, &credentials) {
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
