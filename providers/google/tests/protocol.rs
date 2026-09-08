// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use permesh_provider_protocol::{BrowserAuthDecoder, Progress, SetupDecoder};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

async fn process(input: &[u8]) -> std::process::Output {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-google"))
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
    let result=process(b"{\"protocol\":3,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"google-main\"}\n{\"protocol\":3,\"id\":\"describe\",\"method\":\"describe\"}\n").await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let mut decoder = SetupDecoder::new(
        "google",
        "google-main",
        Some(&permesh_provider_google::provider_metadata().capabilities),
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
        ["customer_id", "auth_mode", "token"]
    );
}
#[tokio::test]
async fn real_binary_rejects_malformed_and_unknown_fields_without_echo() {
    for input in [&b"{bad SECRET}\n"[..], &b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"google-main\",\"secret\":\"SECRET\"}\n"[..]] {
        let result=process(input).await; assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("SECRET"));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("SECRET"));
        assert!(String::from_utf8_lossy(&result.stdout).contains("protocol_error"));
    }
}
#[tokio::test]
async fn real_binary_cancels_before_network_when_cancel_is_queued() {
    let input=b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"google-main\"}\n{\"protocol\":2,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{\"customer_id\":\"C123\"},\"credentials\":{\"token\":\"SECRET\"}}\n{\"protocol\":2,\"id\":\"cancel\",\"method\":\"cancel\"}\n";
    let result = process(input).await;
    assert!(String::from_utf8_lossy(&result.stdout).contains("cancelled"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SECRET"));
}

#[tokio::test]
async fn real_binary_exits_after_describe_even_when_stdin_is_kept_open() {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-google"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"{\"protocol\":3,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"google-main\"}\n{\"protocol\":3,\"id\":\"describe\",\"method\":\"describe\"}\n").await.unwrap();
    let output = tokio::time::timeout(std::time::Duration::from_secs(3), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    drop(stdin);
}
#[tokio::test]
async fn real_binary_request_deadline_exits_with_stdin_kept_open() {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-google"))
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

#[tokio::test]
async fn real_binary_cancels_refresh_before_token_exchange_and_keeps_secrets_off_output() {
    let input=b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"directory\"}\n{\"protocol\":2,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{\"customer_id\":\"C123\",\"auth_mode\":\"refresh_token\",\"client_id\":\"desktop.apps.googleusercontent.com\"},\"credentials\":{\"refresh_token\":\"REFRESH-SENTINEL\",\"client_secret\":\"CLIENT-SENTINEL\"}}\n{\"protocol\":2,\"id\":\"cancel\",\"method\":\"cancel\"}\n";
    let result = process(input).await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("cancelled"));
    assert!(!output.contains("SENTINEL"));
}

#[tokio::test]
async fn browser_auth_description_is_reference_only_and_read_only() {
    let input = b"{\"protocol\":4,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"directory\"}\n{\"protocol\":4,\"id\":\"describe_auth\",\"method\":\"describe_auth\"}\n";
    let result = process(input).await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let mut decoder = BrowserAuthDecoder::new(
        "google",
        "directory",
        Some(&permesh_provider_google::provider_metadata().capabilities),
    )
    .unwrap();
    for frame in result.stdout.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    decoder.finish().unwrap().validate().unwrap();
    let frames: Vec<serde_json::Value> = result
        .stdout
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["protocol"], 4);
    assert_eq!(frames[1]["event"], "auth");
    assert_eq!(frames[1]["id"], "describe_auth");
    let spec = &frames[1]["spec"];
    assert_eq!(spec["schema_version"], 1);
    assert_eq!(
        spec["authorization_endpoint"],
        "https://accounts.google.com/o/oauth2/v2/auth"
    );
    assert_eq!(
        spec["token_endpoint"],
        "https://oauth2.googleapis.com/token"
    );
    assert_eq!(
        spec["scopes"],
        serde_json::json!(["https://www.googleapis.com/auth/admin.directory.user.readonly"])
    );
    assert_eq!(spec["client_id_field"], "client_id");
    assert_eq!(spec["client_secret_slot"], "client_secret");
    assert_eq!(spec["refresh_token_slot"], "refresh_token");
    assert_eq!(
        spec["when"],
        serde_json::json!({"field":"auth_mode","equals":"refresh_token"})
    );
    assert_eq!(
        spec["authorization_parameters"],
        serde_json::json!({"access_type":"offline","prompt":"consent"})
    );
}

#[tokio::test]
async fn browser_description_rejects_credentials_and_does_not_enable_draft4_discovery() {
    for tail in [
        serde_json::json!({"protocol":4,"id":"describe_auth","method":"describe_auth","credentials":{"token":"NEVER-ECHO"}}),
        serde_json::json!({"protocol":4,"id":"discover","method":"discover","configuration":{"customer_id":"C123"},"credentials":{"token":"NEVER-ECHO"}}),
    ] {
        let input = format!(
            "{{\"protocol\":4,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"directory\"}}\n{tail}\n"
        );
        let result = process(input.as_bytes()).await;
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("NEVER-ECHO"));
        assert!(!String::from_utf8_lossy(&result.stderr).contains("NEVER-ECHO"));
    }
}
