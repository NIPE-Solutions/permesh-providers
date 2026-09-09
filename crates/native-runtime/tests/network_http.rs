// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use permesh_native_runtime::network::client_builder;
use permesh_provider_sdk::network::NetworkContext;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
const ROOT: &str = include_str!("fixtures/network/root.pem");
async fn tls_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let certs = CertificateDer::pem_slice_iter(include_bytes!("fixtures/network/server.pem"))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(include_bytes!("fixtures/network/server.key")).unwrap();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(mut stream) = acceptor.accept(stream).await {
                    let mut bytes = [0; 8192];
                    if stream.read(&mut bytes).await.is_ok() {
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").await;
                        let _ = stream.shutdown().await;
                    }
                }
            });
        }
    });
    (address, task)
}
async fn proxy(target: SocketAddr) -> (SocketAddr, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let task = tokio::spawn(async move {
        while let Ok((mut client, _)) = listener.accept().await {
            observed.fetch_add(1, Ordering::SeqCst);
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") && header.len() < 8192 {
                let mut byte = [0];
                if client.read_exact(&mut byte).await.is_err() {
                    break;
                }
                header.push(byte[0]);
            }
            assert!(
                String::from_utf8_lossy(&header)
                    .starts_with(&format!("CONNECT localhost:{} HTTP/1.1", target.port()))
            );
            let mut upstream = TcpStream::connect(target).await.unwrap();
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            tokio::spawn(async move {
                let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            });
        }
    });
    (address, calls, task)
}
fn context() -> NetworkContext {
    NetworkContext {
        ca_bundle_pem: Some(ROOT.into()),
        ..Default::default()
    }
}
#[tokio::test]
async fn explicit_ca_enables_tls_but_never_disables_hostname_verification() {
    let (address, server) = tls_server().await;
    let url = format!("https://localhost:{}/", address.port());
    let untrusted = client_builder(None)
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(untrusted.get(&url).send().await.is_err());
    let trusted = client_builder(Some(&context()))
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert_eq!(
        trusted
            .get(&url)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );
    let wrong = client_builder(Some(&context()))
        .unwrap()
        .resolve("wrong.invalid", address)
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(
        wrong
            .get(format!("https://wrong.invalid:{}/", address.port()))
            .send()
            .await
            .is_err()
    );
    server.abort();
}
#[tokio::test]
async fn explicit_https_proxy_tunnels_tls_and_no_proxy_bypasses_only_selected_host() {
    let (address, server) = tls_server().await;
    let (proxy_address, calls, proxy_task) = proxy(address).await;
    let mut network = context();
    network.https_proxy = Some(format!("http://{proxy_address}"));
    let url = format!("https://localhost:{}/", address.port());
    let routed = client_builder(Some(&network))
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert_eq!(
        routed.get(&url).send().await.unwrap().text().await.unwrap(),
        "ok"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    network.no_proxy.push("other.invalid".into());
    let not_bypassed = client_builder(Some(&network))
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert_eq!(
        not_bypassed
            .get(&url)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    network.no_proxy = vec!["localhost".into()];
    let direct = client_builder(Some(&network))
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert_eq!(
        direct.get(&url).send().await.unwrap().text().await.unwrap(),
        "ok"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    proxy_task.abort();
    server.abort();
}
#[tokio::test]
async fn malformed_or_private_ca_is_rejected_before_any_request() {
    for pem in [
        "",
        "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n",
        "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
    ] {
        let network = NetworkContext {
            ca_bundle_pem: Some(pem.into()),
            ..Default::default()
        };
        assert!(client_builder(Some(&network)).is_err());
    }
    let mixed = NetworkContext {
        ca_bundle_pem: Some(format!(
            "{ROOT}-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n"
        )),
        ..Default::default()
    };
    assert!(client_builder(Some(&mixed)).is_err());
}
#[tokio::test]
async fn hostile_ambient_proxy_settings_are_not_inherited() {
    let (address, server) = tls_server().await;
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let trap_url = format!("http://{}", trap.local_addr().unwrap());
    let output = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "ambient_environment_child", "--nocapture"])
        .env(
            "PERMESH_NETWORK_TEST_URL",
            format!("https://localhost:{}/", address.port()),
        )
        .env("HTTPS_PROXY", &trap_url)
        .env("https_proxy", &trap_url)
        .env("HTTP_PROXY", &trap_url)
        .env("http_proxy", &trap_url)
        .env("ALL_PROXY", &trap_url)
        .env("all_proxy", &trap_url)
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), trap.accept())
            .await
            .is_err()
    );
    server.abort();
}
#[tokio::test]
async fn ambient_environment_child() {
    let Ok(url) = std::env::var("PERMESH_NETWORK_TEST_URL") else {
        return;
    };
    let client = client_builder(Some(&context()))
        .unwrap()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert_eq!(
        client.get(url).send().await.unwrap().text().await.unwrap(),
        "ok"
    );
}
