// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use permesh_native_runtime::{Adapter, serve};
use permesh_provider_sdk::{Health, Metadata, Provider, ProviderFuture, setup::SetupSpec};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
struct Fixture;
impl Adapter for Fixture {
    type Configuration = Empty;
    type Credentials = Empty;
    fn metadata() -> Metadata {
        Metadata {
            kind: "fixture".into(),
            capabilities: vec![],
        }
    }
    fn setup() -> SetupSpec {
        panic!("not setup")
    }
    fn validate(_: &Empty, _: &Empty) -> bool {
        true
    }
    fn error_code(_: &str) -> &'static str {
        "internal"
    }
    fn limitations(_: &[String]) -> Vec<&'static str> {
        vec![]
    }
}
impl Provider for Fixture {
    fn metadata(&self) -> Metadata {
        <Self as Adapter>::metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            Ok(Health {
                message: "ok".into(),
                limitations: vec![],
            })
        })
    }
    fn discover(&self) -> ProviderFuture<'_, permesh_core::Snapshot> {
        Box::pin(async { Ok(permesh_core::Snapshot::new("test")) })
    }
}
#[tokio::test]
async fn unrequested_network_must_never_reach_legacy_factory() {
    let (mut host, input) = tokio::io::duplex(8192);
    host.write_all(b"{\"protocol_version\":1,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\",\"operation\":\"check\"}\n{\"protocol_version\":1,\"id\":\"check\",\"method\":\"check\",\"configuration\":{},\"credentials\":{},\"network\":{\"https_proxy\":\"http://127.0.0.1:1234\"}}\n").await.unwrap();
    let mut output = vec![];
    assert!(
        serve::<Fixture, _, _, _, _, Fixture>(input, &mut output, |_, _, _| async {
            panic!("network ignored by factory")
        })
        .await
        .is_err()
    );
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("protocol_error")
    );
}

#[tokio::test]
async fn network_feature_is_opt_in_and_delivers_validated_context() {
    let (mut host, input) = tokio::io::duplex(8192);
    host.write_all(b"{\"protocol_version\":1,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\",\"operation\":\"check\",\"features\":[\"network_v1\"]}\n{\"protocol_version\":1,\"id\":\"check\",\"method\":\"check\",\"configuration\":{},\"credentials\":{},\"network\":{\"https_proxy\":\"http://127.0.0.1:1234\"}}\n").await.unwrap();
    let mut output = vec![];
    assert!(
        permesh_native_runtime::serve_with_network::<NetworkFixture, _, _, _, _, _>(
            input,
            &mut output,
            |_, _, _, network| async move {
                assert_eq!(
                    network.unwrap().https_proxy.as_deref(),
                    Some("http://127.0.0.1:1234")
                );
                Ok(Fixture)
            }
        )
        .await
        .is_ok()
    );
    let frames: Vec<serde_json::Value> = output
        .split_inclusive(|b| *b == b'\n')
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect();
    assert_eq!(frames[0]["features"], serde_json::json!(["network_v1"]));
    assert_eq!(frames[0]["capabilities"], serde_json::json!([]));
    assert_eq!(frames[1]["event"], "health");
    let mut decoder = permesh_provider_protocol::negotiated::HealthDecoder::with_required_features(
        "fixture",
        "test",
        Some(&[]),
        &[permesh_provider_protocol::negotiated::Feature::NetworkV1],
    )
    .unwrap();
    for frame in output.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    decoder.finish().unwrap();
}
struct NetworkFixture;
impl Adapter for NetworkFixture {
    type Configuration = Empty;
    type Credentials = Empty;
    fn supports_network() -> bool {
        true
    }
    fn metadata() -> Metadata {
        <Fixture as Adapter>::metadata()
    }
    fn setup() -> SetupSpec {
        panic!("not setup")
    }
    fn validate(_: &Empty, _: &Empty) -> bool {
        true
    }
    fn error_code(_: &str) -> &'static str {
        "internal"
    }
    fn limitations(_: &[String]) -> Vec<&'static str> {
        vec![]
    }
}

#[tokio::test]
async fn old_factory_api_rejects_network_even_when_adapter_claims_support() {
    let (mut host, input) = tokio::io::duplex(8192);
    host.write_all(b"{\"protocol_version\":1,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\",\"operation\":\"check\",\"features\":[\"network_v1\"]}\n").await.unwrap();
    let mut output = vec![];
    assert!(
        serve::<NetworkFixture, _, _, _, _, Fixture>(input, &mut output, |_, _, _| async {
            panic!("old factory ignored network")
        })
        .await
        .is_err()
    );
    let response: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(response["event"], "error");
}
#[tokio::test]
async fn malformed_or_unnegotiated_context_never_reaches_network_factory() {
    let handshake = serde_json::json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":"check","features":["network_v1"]});
    let invocation = serde_json::json!({"protocol_version":1,"id":"check","method":"check","configuration":{},"credentials":{},"network":{"https_proxy":"http://127.0.0.1:1234"}});
    let mut cases = vec![];
    for features in [
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!(["unknown"]),
        serde_json::json!(["network_v1", "network_v1"]),
    ] {
        let mut altered = handshake.clone();
        altered["features"] = features;
        cases.push((altered, invocation.clone()));
    }
    for network in [
        serde_json::Value::Null,
        serde_json::json!({}),
        serde_json::json!({"https_proxy":"http://user:private-password@proxy.invalid"}),
        serde_json::json!({"https_proxy":"http://127.0.0.1:1234","unknown":true}),
        serde_json::json!({"ca_bundle_pem":"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n"}),
    ] {
        let mut altered = invocation.clone();
        altered["network"] = network;
        cases.push((handshake.clone(), altered));
    }
    let mut no_network = invocation.clone();
    no_network.as_object_mut().unwrap().remove("network");
    cases.push((handshake.clone(), no_network));
    let mut unrequested = handshake.clone();
    unrequested.as_object_mut().unwrap().remove("features");
    cases.push((unrequested, invocation.clone()));
    let mut legacy = handshake.clone();
    legacy.as_object_mut().unwrap().remove("protocol_version");
    legacy["protocol"] = serde_json::json!(3);
    cases.push((legacy, invocation));
    for (handshake, invocation) in cases {
        let (mut host, input) = tokio::io::duplex(8192);
        host.write_all(format!("{handshake}\n{invocation}\n").as_bytes())
            .await
            .unwrap();
        let mut output = vec![];
        assert!(
            permesh_native_runtime::serve_with_network::<NetworkFixture, _, _, _, _, Fixture>(
                input,
                &mut output,
                |_, _, _, _| async { panic!("invalid network reached factory") }
            )
            .await
            .is_err()
        );
        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("protocol_error"));
        assert!(!text.contains("private-password"));
    }
}

#[test]
fn duplicate_network_and_feature_fields_are_rejected() {
    let handshake = br#"{"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":"check","features":["network_v1"],"features":["network_v1"]}"#;
    let invocation = br#"{"protocol_version":1,"id":"check","method":"check","configuration":{},"credentials":{},"network":{"https_proxy":"http://127.0.0.1:1234"},"network":{"https_proxy":"http://127.0.0.1:5678"}}"#;
    assert!(permesh_native_runtime::validate_request::<NetworkFixture>(handshake).is_err());
    assert!(permesh_native_runtime::validate_request::<NetworkFixture>(invocation).is_err());
}
