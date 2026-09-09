// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use permesh_provider_protocol::{Progress, SetupDecoder};
use serde_json::json;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
async fn process(input: &[u8]) -> std::process::Output {
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_permesh-provider-cloudflare"))
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
        json!({"protocol":3,"id":"handshake","method":"handshake","instance":"cf"}),
        json!({"protocol":3,"id":"describe","method":"describe"})
    );
    let result = process(input.as_bytes()).await;
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let mut decoder = SetupDecoder::new(
        "cloudflare",
        "cf",
        Some(&permesh_provider_cloudflare::provider_metadata().capabilities),
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
        ["account_id", "token"]
    );
}
#[tokio::test]
async fn binary_rejects_credentials_in_configuration_and_unknown_fields_without_reflection() {
    let input = format!(
        "{}\n{}\n",
        json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"cf","operation":"discover"}),
        json!({"protocol_version":1,"id":"discover","method":"discover","configuration":{"account_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","token":"SENTINEL"},"credentials":{"token":"SENTINEL"}})
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
        json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"cf","operation":"discover"}),
        json!({"protocol_version":1,"id":"discover","method":"discover","configuration":{"account_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"credentials":{"token":"SENTINEL"}}),
        json!({"protocol_version":1,"id":"cancel","method":"cancel"})
    );
    let result = process(input.as_bytes()).await;
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("cancelled"));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("SENTINEL"));
}

#[tokio::test]
async fn binary_rejects_draft_discovery_and_unselected_operations_before_credentials() {
    for handshake in [
        json!({"protocol":2,"id":"handshake","method":"handshake","instance":"cf"}),
        json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"cf"}),
        json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"cf","operation":"describe"}),
    ] {
        let result = process(format!("{handshake}\n").as_bytes()).await;
        assert!(!result.status.success());
        assert!(result.stderr.is_empty());
        assert!(String::from_utf8_lossy(&result.stdout).contains("protocol_error"));
    }
}
