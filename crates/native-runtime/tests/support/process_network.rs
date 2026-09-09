// SPDX-License-Identifier: MIT
// Shared subprocess fixture: HTTPS CONNECT is intercepted locally and rejected.
// The target provider endpoints are never contacted and no API token leaves TLS.
pub async fn check_proxy(
    binary: &str,
    instance: &str,
    configuration: serde_json::Value,
    credentials: serde_json::Value,
    expected_host: &str,
) {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let (observed, connected) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") && header.len() < 8192 {
            let mut byte = [0];
            socket.read_exact(&mut byte).await.unwrap();
            header.push(byte[0]);
        }
        observed.send(header).unwrap();
        socket.write_all(b"HTTP/1.1 502 Local Test Rejection\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let mut child = tokio::process::Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("https_proxy", "http://127.0.0.1:1")
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let handshake = serde_json::json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":instance,"operation":"check","features":["network_v1"]});
    let invocation = serde_json::json!({"protocol_version":1,"id":"check","method":"check","configuration":configuration,"credentials":credentials,"network":{"https_proxy":proxy}});
    stdin
        .write_all(format!("{handshake}\n{invocation}\n").as_bytes())
        .await
        .unwrap();
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    drop(stdin);
    let header = tokio::time::timeout(std::time::Duration::from_secs(1), connected)
        .await
        .unwrap()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&header)
            .starts_with(&format!("CONNECT {expected_host}:443 HTTP/1.1"))
    );
    assert!(!String::from_utf8_lossy(&header).contains("SENTINEL"));
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("SENTINEL"));
    let first = output
        .stdout
        .split_inclusive(|b| *b == b'\n')
        .next()
        .unwrap();
    let handshake: serde_json::Value = serde_json::from_slice(first).unwrap();
    assert_eq!(handshake["features"], serde_json::json!(["network_v1"]));
    server.await.unwrap();
}
