// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
#[tokio::test]
async fn native_setup_and_cancel_are_offline_and_never_reflect_input() {
    for describe in [true, false] {
        let mut child = tokio::process::Command::new(env!(
            "CARGO_BIN_EXE_permesh-provider-aws-identity-center"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let handshake = if describe {
            serde_json::json!({"protocol":3,"id":"handshake","method":"handshake","instance":"test-main"})
        } else {
            serde_json::json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"test-main","operation":"discover"})
        };
        input
            .write_all(format!("{handshake}\n").as_bytes())
            .await
            .unwrap();
        input.flush().await.unwrap();
        let mut first = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            output.read_line(&mut first),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(first.contains("aws-identity-center"));
        let request = if describe {
            serde_json::json!({"protocol":3,"id":"describe","method":"describe"})
        } else {
            serde_json::json!({"protocol_version":1,"id":"cancel","method":"cancel"})
        };
        input
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        input.flush().await.unwrap();
        let mut second = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            output.read_line(&mut second),
        )
        .await
        .unwrap()
        .unwrap();
        if describe {
            let mut decoder = permesh_provider_protocol::SetupDecoder::new(
                "aws-identity-center",
                "test-main",
                Some(&permesh_provider_aws::identity_center::provider_metadata().capabilities),
            )
            .unwrap();
            decoder.push_frame(first.as_bytes()).unwrap();
            decoder.push_frame(second.as_bytes()).unwrap();
            decoder.finish().unwrap().validate().unwrap();
        } else {
            assert!(second.contains("cancelled"));
        }
        drop(input);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), child.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        let mut stderr = Vec::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_end(&mut stderr)
            .await
            .unwrap();
        assert!(stderr.is_empty());
    }
}
#[tokio::test]
async fn malformed_native_credential_request_returns_only_curated_failure() {
    let mut child =
        tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-aws-identity-center"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
    child.stdin.as_mut().unwrap().write_all(b"{\"protocol_version\":1,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test-main\",\"operation\":\"discover\"}\n{\"protocol_version\":1,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{\"origin\":\"http://unapproved.example\",\"group_ids\":[\"10\"]},\"credentials\":{\"token\":\"SYNTHETIC_PRIVATE_SENTINEL\"}}\n").await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SYNTHETIC_PRIVATE_SENTINEL"));
    assert!(result.stderr.is_empty());
}

#[path = "../../../crates/native-runtime/tests/support/process_network.rs"]
mod process_network;
#[tokio::test]
async fn explicit_network_routes_signed_sts_through_approved_proxy() {
    process_network::check_proxy(
        env!("CARGO_BIN_EXE_permesh-provider-aws-identity-center"),
        "center",
        serde_json::json!({"account_id":"111111111111","region":"eu-west-1","instance_arn":"arn:aws:sso:::instance/ssoins-1234567890123456","identity_store_id":"d-1234567890","accounts":["222222222222"]}),
        serde_json::json!({"access_key_id":"ASIATEST1234567890123","secret_access_key":"SYNTHETIC_SECRET","session_token":"SYNTHETIC_SESSION"}),
        "sts.eu-west-1.amazonaws.com",
    ).await;
}
