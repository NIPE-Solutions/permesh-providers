// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used)]
use permesh_core::Snapshot;
use permesh_native_runtime::{Adapter, serve, validate_request};
use permesh_provider_sdk::{Health, Metadata, Provider, ProviderFuture, setup::SetupSpec};
use serde::Deserialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncWriteExt;
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    token: Zeroizing<String>,
}
struct Fixture;
impl Adapter for Fixture {
    type Configuration = Configuration;
    type Credentials = Credentials;
    fn metadata() -> Metadata {
        Metadata {
            kind: "fixture".into(),
            capabilities: vec![],
        }
    }
    fn setup() -> SetupSpec {
        panic!("not a setup operation")
    }
    fn validate(_: &Configuration, credentials: &Credentials) -> bool {
        !credentials.token.is_empty()
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
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        Box::pin(async { Ok(Snapshot::new("test")) })
    }
}
fn handshake(operation: &str) -> String {
    format!(
        r#"{{"protocol_version":1,"id":"handshake","method":"handshake","instance":"test","operation":"{operation}"}}"#
    )
}
fn invocation(operation: &str) -> String {
    format!(
        r#"{{"protocol_version":1,"id":"{operation}","method":"{operation}","configuration":{{}},"credentials":{{"token":"private-canary"}}}}"#
    )
}
async fn transcript(frames: &[String], allow_factory: bool) -> (bool, Vec<serde_json::Value>) {
    let (mut input, reader) = tokio::io::duplex(8192);
    input
        .write_all(format!("{}\n", frames.join("\n")).as_bytes())
        .await
        .expect("input");
    let mut output = Vec::new();
    let called = AtomicBool::new(false);
    let result = serve::<Fixture, _, _, _, _, _>(reader, &mut output, |_, _, _| async {
        assert!(allow_factory, "invalid transcript reached factory");
        called.store(true, Ordering::SeqCst);
        Ok(Fixture)
    })
    .await;
    assert_eq!(called.load(Ordering::SeqCst), allow_factory);
    assert!(!String::from_utf8_lossy(&output).contains("private-canary"));
    (
        result.is_ok(),
        output
            .split_inclusive(|b| *b == b'\n')
            .map(|line| serde_json::from_slice(line).expect("output JSON"))
            .collect(),
    )
}

#[tokio::test]
async fn selected_operation_and_family_are_enforced_before_factory() {
    for (selected, invoked) in [("check", "discover"), ("discover", "check")] {
        let (ok, frames) = transcript(&[handshake(selected), invocation(invoked)], false).await;
        assert!(!ok);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1]["code"], "protocol_error");
    }
    for invalid in [
        r#"{"protocol":2,"id":"handshake","method":"handshake","instance":"test"}"#.into(),
        invocation("check"),
        handshake("describe"),
        handshake("check").replace("\"protocol_version\":1", "\"protocol_version\":99"),
    ] {
        let (ok, frames) = transcript(&[invalid], false).await;
        assert!(!ok);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["event"], "error");
    }
    for frame in [
        invocation("discover").replace("\"protocol_version\":1", "\"protocol\":2"),
        invocation("discover").replace("\"protocol_version\":1", "\"protocol\":3"),
        r#"{"protocol":3,"id":"describe","method":"describe"}"#.into(),
        r#"{"protocol":4,"id":"describe_auth","method":"describe_auth"}"#.into(),
    ] {
        assert!(!transcript(&[handshake("discover"), frame], false).await.0);
    }
}

#[tokio::test]
async fn matching_operations_cross_decode_and_advertise_exact_capabilities() {
    for operation in ["check", "discover"] {
        let (ok, frames) = transcript(&[handshake(operation), invocation(operation)], true).await;
        assert!(ok);
        assert_eq!(
            frames[0]["operations"],
            serde_json::json!(["discover", "check"])
        );
        assert_eq!(frames[0]["capabilities"], serde_json::json!([]));
        assert_eq!(frames[0]["protocol_version"], 1);
        assert!(frames.iter().all(|frame| frame.get("protocol").is_none()));
        if operation == "check" {
            let mut decoder = permesh_provider_protocol::negotiated::HealthDecoder::new(
                "fixture",
                "test",
                Some(&[]),
            )
            .expect("decoder");
            for frame in frames {
                let mut bytes = serde_json::to_vec(&frame).expect("frame");
                bytes.push(b'\n');
                decoder.push_frame(&bytes).expect("decode");
            }
            decoder.finish().expect("health");
        } else {
            let mut decoder = permesh_provider_protocol::negotiated::DiscoveryDecoder::new(
                "fixture",
                "test",
                Some(&[]),
            )
            .expect("decoder");
            for frame in frames {
                let mut bytes = serde_json::to_vec(&frame).expect("frame");
                bytes.push(b'\n');
                decoder.push_frame(&bytes).expect("decode");
            }
            decoder.finish().expect("snapshot");
        }
    }
}

#[test]
fn credential_frames_reject_null_unknown_duplicate_and_mixed_fields() {
    let valid = invocation("discover");
    assert!(validate_request::<Fixture>(valid.as_bytes()).is_ok());
    for invalid in [
        valid.replace(
            "\"protocol_version\":1",
            "\"protocol_version\":1,\"protocol\":2",
        ),
        valid.replace(
            "\"protocol_version\":1",
            "\"protocol_version\":1,\"protocol_version\":1",
        ),
        valid.replace("\"configuration\":{}", "\"configuration\":null"),
        valid.replace(
            "\"configuration\":{}",
            "\"configuration\":{},\"configuration\":{}",
        ),
        valid.replace(
            "\"configuration\":{}",
            "\"configuration\":{\"unknown\":true}",
        ),
        valid.replace(
            "\"credentials\":{\"token\":\"private-canary\"}",
            "\"credentials\":null",
        ),
        valid.replace(
            "\"token\":\"private-canary\"",
            "\"token\":\"private-canary\",\"token\":\"private-canary\"",
        ),
        valid.replace(
            "\"configuration\":{}",
            "\"configuration\":{},\"operation\":\"discover\"",
        ),
        valid.replace(
            "\"configuration\":{}",
            "\"configuration\":{},\"unknown\":\"private-canary\"",
        ),
    ] {
        assert!(validate_request::<Fixture>(invalid.as_bytes()).is_err());
    }
}

#[tokio::test]
async fn only_same_family_cancel_is_accepted_before_factory() {
    for protocol in [
        r#""protocol_version":1"#,
        r#""protocol":3"#,
        r#""protocol":4"#,
    ] {
        let cancel = format!(r#"{{{protocol},"id":"cancel","method":"cancel"}}"#);
        let (ok, frames) = transcript(&[handshake("discover"), cancel.clone()], false).await;
        assert_eq!(ok, protocol.contains("protocol_version"));
        assert_eq!(frames[1]["event"], if ok { "cancelled" } else { "error" });
        let (ok_queued, queued_frames) = transcript(
            &[handshake("discover"), invocation("discover"), cancel],
            false,
        )
        .await;
        assert_eq!(ok_queued, ok);
        assert_eq!(queued_frames[1]["event"], frames[1]["event"]);
    }
}

struct DropSignal(Arc<AtomicBool>);
impl Drop for DropSignal {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[tokio::test]
async fn in_flight_cancel_drops_factory_future() {
    let (mut input, reader) = tokio::io::duplex(8192);
    input
        .write_all(format!("{}\n{}\n", handshake("discover"), invocation("discover")).as_bytes())
        .await
        .expect("input");
    let (started, ready) = tokio::sync::oneshot::channel();
    let dropped = Arc::new(AtomicBool::new(false));
    let mut output = Vec::new();
    let server =
        serve::<Fixture, _, _, _, _, Fixture>(reader, &mut output, |_, _, credentials| async {
            let _signal = DropSignal(dropped.clone());
            let _credentials = credentials;
            started.send(()).expect("started");
            std::future::pending().await
        });
    let cancel = async {
        ready.await.expect("factory started");
        input
            .write_all(b"{\"protocol_version\":1,\"id\":\"cancel\",\"method\":\"cancel\"}\n")
            .await
            .expect("cancel");
    };
    let (result, ()) = tokio::join!(server, cancel);
    assert!(result.is_ok());
    assert!(dropped.load(Ordering::SeqCst));
    assert!(String::from_utf8_lossy(&output).contains("cancelled"));
    assert!(!String::from_utf8_lossy(&output).contains("private-canary"));
}

#[tokio::test(start_paused = true)]
async fn negotiated_operation_deadline_drops_private_factory_state() {
    let (mut input, reader) = tokio::io::duplex(8192);
    input
        .write_all(format!("{}\n{}\n", handshake("discover"), invocation("discover")).as_bytes())
        .await
        .expect("input");
    let dropped = Arc::new(AtomicBool::new(false));
    let mut output = Vec::new();
    let started = tokio::time::Instant::now();
    let result =
        serve::<Fixture, _, _, _, _, Fixture>(reader, &mut output, |_, _, credentials| async {
            let _signal = DropSignal(dropped.clone());
            let _credentials = credentials;
            std::future::pending().await
        })
        .await;
    assert!(result.is_err());
    assert_eq!(started.elapsed(), std::time::Duration::from_secs(55));
    assert!(dropped.load(Ordering::SeqCst));
    assert!(String::from_utf8_lossy(&output).contains("unavailable"));
    assert!(!String::from_utf8_lossy(&output).contains("private-canary"));
}

#[tokio::test]
async fn legacy_setup_and_auth_keep_their_cancel_family() {
    for version in [3, 4] {
        let first = format!(
            r#"{{"protocol":{version},"id":"handshake","method":"handshake","instance":"test"}}"#
        );
        for family in [
            r#""protocol_version":1"#.to_owned(),
            r#""protocol":3"#.to_owned(),
            r#""protocol":4"#.to_owned(),
        ] {
            let cancel = format!(r#"{{{family},"id":"cancel","method":"cancel"}}"#);
            let (ok, frames) = transcript(&[first.clone(), cancel], false).await;
            assert_eq!(ok, family == format!("\"protocol\":{version}"));
            assert_eq!(frames[0]["protocol"], version);
            assert!(frames[0].get("protocol_version").is_none());
            assert!(frames[0].get("operations").is_none());
            assert_eq!(frames[1]["event"], if ok { "cancelled" } else { "error" });
            assert_eq!(frames[1]["protocol"], version);
        }
    }
}
