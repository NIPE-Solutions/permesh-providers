// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used)]
use super::*;
use permesh_provider_protocol::{DiscoveryDecoder, HealthDecoder};
use permesh_provider_sdk::Provider;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn mock<F>(handler: F) -> (String, tokio::task::JoinHandle<()>)
where
    F: Fn(&str) -> (u16, String) + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = vec![0; 8192];
            let n = socket.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..n]);
            let path = text.split_whitespace().nth(1).unwrap();
            let (status, body) = handler(path);
            let reply = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    (origin, task)
}
fn provider(origin: &str) -> GithubProvider {
    let mut p = GithubProvider::new(
        "github-main".into(),
        vec!["acme".into()],
        Secret::new("SENTINEL-secret".into()),
    )
    .unwrap();
    p.origin = reqwest::Url::parse(origin).unwrap();
    p
}
async fn exchange(origin: &str, check: bool) -> (Result<(), ProtocolFailure>, Vec<u8>) {
    let (mut host, peer) = tokio::io::duplex(128 * 1024);
    let (reader, writer) = tokio::io::split(peer);
    let origin = origin.to_owned();
    let session = tokio::spawn(async move {
        serve(reader, writer, move |id, orgs, token| {
            let mut p = GithubProvider::new(id, orgs, token)?;
            p.origin = reqwest::Url::parse(&origin).map_err(|_| crate::error("configuration"))?;
            Ok(p)
        })
        .await
    });
    let method = if check { "check" } else { "discover" };
    let request = format!(
        "{{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"github-main\"}}\n{{\"protocol\":2,\"id\":\"{method}\",\"method\":\"{method}\",\"configuration\":{{\"organizations\":[\"acme\"]}},\"credentials\":{{\"token\":\"SENTINEL-secret\"}}}}\n"
    );
    host.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    host.read_to_end(&mut response).await.unwrap();
    (session.await.unwrap(), response)
}
fn observations(snapshot: permesh_core::Snapshot) -> Value {
    fn remove_clock(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.remove("observed_at");
                for value in map.values_mut() {
                    remove_clock(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    remove_clock(value);
                }
            }
            _ => (),
        }
    }
    let mut value = serde_json::to_value(snapshot).unwrap();
    value.as_object_mut().unwrap().remove("limitations");
    remove_clock(&mut value);
    value
}
#[tokio::test]
async fn wrapped_discovery_preserves_success_partial_and_pagination_observations() {
    for partial in [false, true] {
        let (origin, task) = mock(move |path| {
            let body = if path == "/orgs/acme" {
                json!({"id":9,"login":"acme"})
            } else if path.starts_with("/orgs/acme/members?role=member") {
                if path.ends_with("page=1") {
                    Value::Array(
                        (1..=100)
                            .map(|id| json!({"id":id,"login":format!("user{id}"),"type":"User"}))
                            .collect(),
                    )
                } else {
                    json!([{"id":101,"login":"last-user","type":"User"}])
                }
            } else if path.starts_with("/orgs/acme/repos?") {
                json!([{"id":7,"name":"app","full_name":"acme/app"}])
            } else if path.starts_with("/repos/acme/app/collaborators?") {
                json!([{"id":1,"login":"user1","type":"User","role_name":"custom-superuser"}])
            } else {
                json!([])
            };
            if partial && path.contains("/teams") {
                (403, "SENTINEL-secret".into())
            } else {
                (200, body.to_string())
            }
        })
        .await;
        let expected = provider(&origin).discover().await.unwrap();
        assert_eq!(expected.accounts.len(), 101);
        assert_eq!(expected.complete, !partial);
        let (status, response) = exchange(&origin, false).await;
        assert!(status.is_ok());
        assert!(!String::from_utf8_lossy(&response).contains("SENTINEL-secret"));
        let mut decoder = DiscoveryDecoder::new_versioned(
            "github",
            "github-main",
            Some(&crate::provider_metadata().capabilities),
            2,
        )
        .unwrap();
        for frame in response.split_inclusive(|b| *b == b'\n') {
            decoder.push_frame(frame).unwrap();
        }
        let actual = decoder.finish().unwrap();
        assert!(
            actual
                .limitations
                .iter()
                .any(|v| v.contains("limited visibility"))
        );
        if partial {
            assert!(
                actual
                    .limitations
                    .iter()
                    .any(|v| v.contains("permission denial"))
            );
        }
        assert_eq!(observations(actual), observations(expected));
        task.abort();
    }
}
#[tokio::test]
async fn wrapped_health_and_error_codes_match_sdk_operations() {
    let (origin, task) = mock(|path| {
        (
            200,
            if path == "/user" {
                r#"{"id":1,"login":"alice"}"#
            } else {
                r#"{"state":"active"}"#
            }
            .into(),
        )
    })
    .await;
    let (status, response) = exchange(&origin, true).await;
    assert!(status.is_ok());
    let mut decoder = HealthDecoder::new(
        "github",
        "github-main",
        Some(&crate::provider_metadata().capabilities),
    )
    .unwrap();
    for frame in response.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    assert!(!decoder.finish().unwrap().limitations.is_empty());
    task.abort();
    for (status, code) in [
        (401, "authentication"),
        (403, "permission_denied"),
        (404, "permission_denied"),
    ] {
        let (origin, task) = mock(move |_| (status, "SENTINEL-secret".into())).await;
        let (result, response) = exchange(&origin, true).await;
        assert!(result.is_err());
        let text = String::from_utf8(response).unwrap();
        assert!(text.contains(code));
        assert!(!text.contains("SENTINEL-secret"));
        task.abort();
    }
}
#[tokio::test]
async fn cancellation_drops_an_in_flight_http_request() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (mut host, peer) = tokio::io::duplex(8192);
    let (reader, writer) = tokio::io::split(peer);
    let session =
        tokio::spawn(
            async move { serve(reader, writer, move |_, _, _| Ok(provider(&origin))).await },
        );
    host.write_all(b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"github-main\"}\n{\"protocol\":2,\"id\":\"check\",\"method\":\"check\",\"configuration\":{\"organizations\":[\"acme\"]},\"credentials\":{\"token\":\"SENTINEL-secret\"}}\n").await.unwrap();
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut request = [0; 8192];
    assert!(socket.read(&mut request).await.unwrap() > 0);
    host.write_all(b"{\"protocol\":2,\"id\":\"cancel\",\"method\":\"cancel\"}\n")
        .await
        .unwrap();
    let mut response = String::new();
    host.read_to_string(&mut response).await.unwrap();
    assert!(session.await.unwrap().is_ok());
    assert!(response.contains("cancelled"));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), socket.read(&mut request))
            .await
            .unwrap()
            .unwrap(),
        0
    );
}

#[tokio::test(start_paused = true)]
async fn abandoned_handshake_has_a_fixed_request_deadline() {
    let (_host, peer) = tokio::io::duplex(8192);
    let (reader, mut writer) = tokio::io::split(peer);
    let mut output = Vec::new();
    let result = serve(reader, &mut output, GithubProvider::new).await;
    assert!(result.is_err());
    assert!(String::from_utf8(output).unwrap().contains("unavailable"));
    writer.shutdown().await.unwrap();
}
#[test]
fn sdk_error_mapping_is_complete_and_fixed() {
    for (code, expected) in [
        ("configuration", "protocol_error"),
        ("unauthorized", "authentication"),
        ("forbidden", "permission_denied"),
        ("membership", "permission_denied"),
        ("not_found", "permission_denied"),
        ("rate_limit", "rate_limited"),
        ("timeout", "unavailable"),
        ("transport", "unavailable"),
        ("unavailable", "unavailable"),
        ("limit", "unavailable"),
        ("discovery", "unavailable"),
        ("clock", "internal"),
        ("malformed", "internal"),
        ("pagination", "internal"),
        ("arbitrary SECRET", "internal"),
    ] {
        assert_eq!(response::error_code(code), expected);
    }
}

#[tokio::test]
async fn conflicting_paginated_observations_never_escape_the_native_contract() {
    for conflict in [
        "identical",
        "account_login",
        "account_type",
        "repository",
        "team",
        "role",
    ] {
        let (origin, task) = mock(move |path| {
            let body = if path == "/orgs/acme" {
                json!({"id":9,"login":"acme"})
            } else if path.starts_with("/orgs/acme/repos?") {
                if path.ends_with("page=1") {
                    Value::Array((0..100).map(|_| json!({"id":7,"full_name":"acme/app"})).collect())
                } else {
                    json!([{"id":7,"full_name":if conflict == "repository" {"acme/renamed"} else {"acme/app"}}])
                }
            } else if path.starts_with("/orgs/acme/teams?") {
                if path.ends_with("page=1") {
                    Value::Array((0..100).map(|_| json!({"id":3,"slug":"eng","name":"Engineering"})).collect())
                } else {
                    json!([{"id":3,"slug":if conflict == "team" {"renamed"} else {"eng"},"name":"Engineering"}])
                }
            } else if path.contains("/teams/eng/members?") {
                json!([{"id":1,"login":"alice","type":"User"}])
            } else if path.contains("/teams/eng/repos?") {
                json!([{"id":7,"full_name":"acme/app"}])
            } else if path == "/orgs/acme/teams/eng/repos/acme/app" {
                json!({"id":7,"full_name":"acme/app","role_name":"write"})
            } else if path.contains("/collaborators?") {
                if path.ends_with("page=1") {
                    Value::Array((0..100).map(|_| json!({"id":1,"login":"alice","type":"User","role_name":"write"})).collect())
                } else {
                    json!([
                        {"id":1,"login":if conflict == "account_login" {"renamed"} else {"alice"},"type":if conflict == "account_type" {"Bot"} else {"User"},"role_name":if conflict == "role" {"admin"} else {"write"}},
                        {"id":1,"login":"alice","type":"User","role_name":"write"}
                    ])
                }
            } else { json!([]) };
            (200, body.to_string())
        }).await;
        let (result, output) = exchange(&origin, false).await;
        assert!(result.is_ok(), "{conflict}");
        let mut decoder = DiscoveryDecoder::new_versioned(
            "github",
            "github-main",
            Some(&crate::provider_metadata().capabilities),
            2,
        )
        .unwrap();
        for frame in output.split_inclusive(|b| *b == b'\n') {
            decoder.push_frame(frame).unwrap();
        }
        let snapshot = decoder.finish().unwrap();
        assert_eq!(snapshot.complete, conflict == "identical", "{conflict}");
        match conflict {
            "identical" => {
                assert_eq!(snapshot.accounts.len(), 1);
                assert_eq!(snapshot.grants.len(), 2);
                assert_eq!(snapshot.memberships.len(), 1);
            }
            "account_login" | "account_type" => {
                assert!(snapshot.accounts.is_empty());
                assert!(snapshot.memberships.is_empty());
                assert!(
                    snapshot
                        .grants
                        .iter()
                        .all(|g| !matches!(g.subject, permesh_core::Subject::Account(_)))
                );
            }
            "repository" => {
                assert!(
                    snapshot
                        .resources
                        .iter()
                        .all(|r| r.key.id != "repository:7")
                );
                assert!(snapshot.grants.is_empty());
            }
            "team" => {
                assert!(snapshot.groups.iter().all(|g| g.key.id != "team:3"));
                assert!(snapshot.memberships.is_empty());
                assert!(
                    snapshot
                        .grants
                        .iter()
                        .all(|g| !matches!(g.subject, permesh_core::Subject::Group(_)))
                );
            }
            "role" => {
                assert!(
                    snapshot
                        .grants
                        .iter()
                        .all(|g| g.provenance.method != "github.repository_collaborator_effective")
                );
            }
            _ => unreachable!(),
        }
        task.abort();
    }
}
