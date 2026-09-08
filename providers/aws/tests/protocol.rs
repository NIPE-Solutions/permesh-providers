// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used)]
use permesh_provider_protocol::{Progress, SetupDecoder};
use serde_json::json;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
async fn process(input: &[u8]) -> std::process::Output {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-aws"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input).await.unwrap();
    drop(stdin);
    tokio::time::timeout(std::time::Duration::from_secs(5), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}
#[tokio::test]
async fn binary_describes_valid_setup_without_credentials_or_network() {
    let input = format!(
        "{}\n{}\n",
        json!({"protocol":3,"id":"handshake","method":"handshake","instance":"aws-main"}),
        json!({"protocol":3,"id":"describe","method":"describe"})
    );
    let result = process(input.as_bytes()).await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let mut decoder = SetupDecoder::new(
        "aws",
        "aws-main",
        Some(&permesh_provider_aws::provider_metadata().capabilities),
    )
    .unwrap();
    let progress: Vec<_> = result
        .stdout
        .split_inclusive(|b| *b == b'\n')
        .map(|f| decoder.push_frame(f).unwrap())
        .collect();
    assert_eq!(progress, [Progress::Handshake, Progress::Complete]);
    let spec = decoder.finish().unwrap();
    spec.validate().unwrap();
    let questions = spec.questions(&Default::default()).unwrap();
    assert_eq!(
        questions
            .iter()
            .map(|q| q.field.key.as_str())
            .collect::<Vec<_>>(),
        [
            "account_id",
            "region",
            "access_key_id",
            "secret_access_key",
            "session_token"
        ]
    );
}
#[tokio::test]
async fn binary_rejects_credentials_in_configuration_and_unknown_fields_without_reflection() {
    let input = format!(
        "{}\n{}\n",
        json!({"protocol":2,"id":"handshake","method":"handshake","instance":"aws-main"}),
        json!({"protocol":2,"id":"discover","method":"discover","configuration":{"account_id":"123456789012","region":"eu-west-1","token":"SENTINEL"},"credentials":{"access_key_id":"AKIATEST1234567890123","secret_access_key":"SENTINEL"}})
    );
    let result = process(input.as_bytes()).await;
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("protocol_error"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SENTINEL"));
    assert!(result.stderr.is_empty());
}
#[tokio::test]
async fn binary_honors_queued_cancellation_before_discovery() {
    let input = format!(
        "{}\n{}\n{}\n",
        json!({"protocol":2,"id":"handshake","method":"handshake","instance":"aws-main"}),
        json!({"protocol":2,"id":"discover","method":"discover","configuration":{"account_id":"123456789012","region":"eu-west-1"},"credentials":{"access_key_id":"AKIATEST1234567890123","secret_access_key":"SENTINEL"}}),
        json!({"protocol":2,"id":"cancel","method":"cancel"})
    );
    let result = process(input.as_bytes()).await;
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("cancelled"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SENTINEL"));
}
