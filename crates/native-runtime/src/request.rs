// SPDX-License-Identifier: MIT
use crate::Adapter;
use permesh_provider_sdk::network::NetworkContext;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(
    deserialize = "A::Configuration: Deserialize<'de>, A::Credentials: Deserialize<'de>"
))]
struct WireRequest<A: Adapter> {
    #[serde(default, deserialize_with = "present")]
    protocol: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    protocol_version: Option<u32>,
    #[serde(default, deserialize_with = "present")]
    operation: Option<Operation>,
    #[serde(default, deserialize_with = "present")]
    features: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    network: Option<NetworkContext>,
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
    Option::<T>::deserialize(d)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("null field"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestContract {
    NegotiatedV1,
    LegacySetup,
    LegacyAuth,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Operation {
    Check,
    Discover,
}
pub(super) struct Request<A: Adapter> {
    pub contract: RequestContract,
    pub command: Command<A>,
}
pub(super) enum Command<A: Adapter> {
    Handshake {
        instance: String,
        operation: Option<Operation>,
        network_requested: bool,
    },
    Operation {
        check: bool,
        configuration: A::Configuration,
        credentials: A::Credentials,
        network: Option<NetworkContext>,
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
    let contract = match (value.protocol, value.protocol_version) {
        (None, Some(1)) => RequestContract::NegotiatedV1,
        (Some(3), None) => RequestContract::LegacySetup,
        (Some(4), None) => RequestContract::LegacyAuth,
        _ => return Err(()),
    };
    if value.id != value.method {
        return Err(());
    }
    if value
        .features
        .as_ref()
        .is_some_and(|features| features.as_slice() != ["network_v1"] || !A::supports_network())
    {
        return Err(());
    }
    if contract != RequestContract::NegotiatedV1
        && (value.features.is_some() || value.network.is_some())
    {
        return Err(());
    }
    if value
        .network
        .as_ref()
        .is_some_and(|network| !A::supports_network() || network.validate().is_err())
    {
        return Err(());
    }
    if !matches!(value.method.as_str(), "check" | "discover") && value.network.is_some() {
        return Err(());
    }
    if value.method != "handshake" && (value.operation.is_some() || value.features.is_some()) {
        return Err(());
    }
    let command = match value.method.as_str() {
        "handshake" if value.configuration.is_none() && value.credentials.is_none() => {
            let instance = value.instance.ok_or(())?;
            if !valid_instance(&instance) {
                return Err(());
            }
            if (contract == RequestContract::NegotiatedV1) != value.operation.is_some() {
                return Err(());
            }
            Command::Handshake {
                instance,
                operation: value.operation,
                network_requested: value.features.is_some(),
            }
        }
        "describe"
            if contract == RequestContract::LegacySetup
                && value.instance.is_none()
                && value.configuration.is_none()
                && value.credentials.is_none() =>
        {
            Command::Describe
        }
        "describe_auth"
            if contract == RequestContract::LegacyAuth
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
        "check" | "discover"
            if contract == RequestContract::NegotiatedV1 && value.instance.is_none() =>
        {
            let configuration = value.configuration.ok_or(())?;
            let credentials = value.credentials.ok_or(())?;
            if !A::validate(&configuration, &credentials) {
                return Err(());
            }
            Command::Operation {
                check: value.method == "check",
                configuration,
                credentials,
                network: value.network,
            }
        }
        _ => return Err(()),
    };
    Ok(Request { contract, command })
}
