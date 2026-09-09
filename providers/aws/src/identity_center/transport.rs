// SPDX-License-Identifier: MIT
//! Bounded transport for official SDK requests; signing stays entirely in the SDK.
use super::{ProviderError, error};
use aws_credential_types::Credentials;
use aws_smithy_runtime_api::client::{
    http::{
        HttpConnector, HttpConnectorFuture, SharedHttpClient, SharedHttpConnector, http_client_fn,
    },
    orchestrator::{HttpRequest, HttpResponse},
    result::ConnectorError,
};
use aws_smithy_types::body::SdkBody;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BODY: usize = 32 * 1024 * 1024;
#[derive(Clone, Debug)]
struct Transport {
    client: reqwest::Client,
    allowed: [String; 4],
    credentials: Credentials,
    attempts: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
}
fn failure() -> ConnectorError {
    ConnectorError::other(
        Box::new(std::io::Error::other("AWS transport failed")),
        None,
    )
}
pub(crate) fn client(
    allowed: [String; 4],
    credentials: Credentials,
    network: Option<&permesh_provider_sdk::network::NetworkContext>,
) -> Result<SharedHttpClient, ProviderError> {
    let client = permesh_native_runtime::network::client_builder(network)?
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| error("configuration"))?;
    let transport = SharedHttpConnector::new(Transport {
        client,
        allowed,
        credentials,
        attempts: Arc::new(AtomicUsize::new(0)),
        bytes: Arc::new(AtomicUsize::new(0)),
    });
    Ok(http_client_fn(move |_, _| transport.clone()))
}
impl HttpConnector for Transport {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let this = self.clone();
        HttpConnectorFuture::new(async move {
            if request.method() != "POST"
                || !this
                    .allowed
                    .iter()
                    .any(|url| request.uri() == url || request.uri() == format!("{url}/"))
                || this.attempts.fetch_add(1, Ordering::Relaxed) >= 1000
            {
                return Err(failure());
            }
            let body = request
                .body()
                .bytes()
                .filter(|b| b.len() <= 64 * 1024)
                .ok_or_else(failure)?
                .to_vec();
            let operation = request
                .headers()
                .get("x-amz-target")
                .and_then(|s| s.rsplit('.').next())
                .map(str::to_owned);
            let field = match operation.as_deref() {
                Some("ListInstances") => Some("Instances"),
                Some("ListUsers") => Some("Users"),
                Some("ListGroups") => Some("Groups"),
                Some("ListGroupMemberships") => Some("GroupMemberships"),
                Some("ListPermissionSetsProvisionedToAccount") => Some("PermissionSets"),
                Some("ListAccountAssignments") => Some("AccountAssignments"),
                Some("ListAccounts") => Some("Accounts"),
                Some("DescribePermissionSet") => None,
                None if body
                    .windows(b"Action=GetCallerIdentity".len())
                    .any(|b| b == b"Action=GetCallerIdentity") =>
                {
                    None
                }
                _ => return Err(failure()),
            };
            let mut builder = this.client.post(request.uri()).body(body);
            for (name, value) in request.headers() {
                builder = builder.header(name, value);
            }
            let mut response = builder.send().await.map_err(|_| failure())?;
            if response
                .content_length()
                .is_some_and(|n| n > MAX_BODY as u64)
            {
                return Err(failure());
            }
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
                if chunk.len() > MAX_BODY.saturating_sub(bytes.len())
                    || this
                        .bytes
                        .fetch_add(chunk.len(), Ordering::Relaxed)
                        .saturating_add(chunk.len())
                        > MAX_TOTAL_BODY
                {
                    return Err(failure());
                }
                bytes.extend_from_slice(&chunk);
            }
            if status == 200 && operation.is_some() {
                let Unique(value) =
                    serde_json::from_slice::<Unique>(&bytes).map_err(|_| failure())?;
                if reflects(&this.credentials, &value)
                    || field.is_some_and(|field| !explicit_array(&bytes, field))
                    || (operation.as_deref() == Some("DescribePermissionSet")
                        && !value.get("PermissionSet").is_some_and(Value::is_object))
                    || value.get("NextToken").is_some_and(|v| {
                        !v.is_null()
                            && v.as_str().is_none_or(|s| {
                                s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control)
                            })
                    })
                {
                    return Err(failure());
                }
            }
            let mut result = HttpResponse::new(
                status.try_into().map_err(|_| failure())?,
                SdkBody::from(bytes),
            );
            for (name, value) in &headers {
                result
                    .headers_mut()
                    .try_insert(
                        name.as_str().to_owned(),
                        value.to_str().map_err(|_| failure())?.to_owned(),
                    )
                    .map_err(|_| failure())?;
            }
            Ok(result)
        })
    }
}

fn reflects(credentials: &Credentials, value: &Value) -> bool {
    match value {
        Value::String(s) => crate::reflects(credentials, s),
        Value::Array(a) => a.iter().any(|v| reflects(credentials, v)),
        Value::Object(o) => o
            .iter()
            .any(|(k, v)| crate::reflects(credentials, k) || reflects(credentials, v)),
        _ => false,
    }
}
use serde::{
    Deserialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique JSON")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Unique, M::Error> {
                let mut out = serde_json::Map::new();
                while let Some((k, Unique(v))) = map.next_entry::<String, Unique>()? {
                    if out.insert(k, v).is_some() {
                        return Err(de::Error::custom("duplicate field"));
                    }
                }
                Ok(Unique(Value::Object(out)))
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Unique, S::Error> {
                let mut out = vec![];
                while let Some(Unique(v)) = seq.next_element()? {
                    out.push(v);
                }
                Ok(Unique(Value::Array(out)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(Value::String(v.into())))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(Value::from(v)))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(Value::from(v)))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                Ok(Unique(Value::from(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
        }
        d.deserialize_any(V)
    }
}
fn explicit_array(bytes: &[u8], field: &str) -> bool {
    serde_json::from_slice::<Unique>(bytes)
        .is_ok_and(|Unique(v)| v.get(field).is_some_and(Value::is_array))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_or_null_list_cannot_be_complete_empty() {
        assert!(explicit_array(br#"{"Users":[]}"#, "Users"));
        for invalid in [
            br#"{}"#.as_slice(),
            br#"{"Users":null}"#,
            br#"{"Users":[],"Users":[]}"#,
        ] {
            assert!(!explicit_array(invalid, "Users"));
        }
    }
}
