// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use permesh_provider_protocol::{Progress, SetupDecoder};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

async fn process(input: &[u8]) -> std::process::Output {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-github"))
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
async fn real_binary_describe_cross_decodes_without_credentials_or_network() {
    let result=process(b"{\"protocol\":3,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"github-main\"}\n{\"protocol\":3,\"id\":\"describe\",\"method\":\"describe\"}\n").await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let mut decoder = SetupDecoder::new(
        "github",
        "github-main",
        Some(&permesh_provider_github::provider_metadata().capabilities),
    )
    .unwrap();
    let progress: Vec<_> = result
        .stdout
        .split_inclusive(|b| *b == b'\n')
        .map(|frame| decoder.push_frame(frame).unwrap())
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
        ["organizations", "token"]
    );
}
#[tokio::test]
async fn real_binary_rejects_malformed_and_unknown_fields_without_echo() {
    for input in [&b"{bad SECRET}\n"[..], &b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"github-main\",\"secret\":\"SECRET\"}\n"[..]] {
        let result=process(input).await; assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("SECRET"));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("SECRET"));
        assert!(String::from_utf8_lossy(&result.stdout).contains("protocol_error"));
    }
}
#[tokio::test]
async fn real_binary_cancels_before_network_when_cancel_is_queued() {
    let input=b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"github-main\"}\n{\"protocol\":2,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{\"organizations\":[\"acme\"]},\"credentials\":{\"token\":\"SECRET\"}}\n{\"protocol\":2,\"id\":\"cancel\",\"method\":\"cancel\"}\n";
    let result = process(input).await;
    assert!(String::from_utf8_lossy(&result.stdout).contains("cancelled"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SECRET"));
}

#[tokio::test]
async fn real_binary_exits_after_describe_even_when_stdin_is_kept_open() {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-github"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"{\"protocol\":3,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"github-main\"}\n{\"protocol\":3,\"id\":\"describe\",\"method\":\"describe\"}\n").await.unwrap();
    let output = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    drop(stdin);
}
#[tokio::test]
async fn real_binary_request_deadline_exits_with_stdin_kept_open() {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-github"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let output = tokio::time::timeout(std::time::Duration::from_secs(12), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("unavailable"));
    drop(stdin);
}
