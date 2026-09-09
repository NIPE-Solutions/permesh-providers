// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use super::*;
#[test]
fn approved_origin_rejects_http_credentials_paths_queries_and_fragments() {
    for value in [
        "http://gitlab.example",
        "https://user:secret@gitlab.example",
        "https://gitlab.example/subpath",
        "https://gitlab.example/?token=secret",
        "https://gitlab.example/#fragment",
        "https://gitlab.example/../",
    ] {
        assert!(origin(value).is_err(), "{value}");
    }
    assert_eq!(
        origin("https://gitlab.example:8443").unwrap().as_str(),
        "https://gitlab.example:8443/"
    );
}
use permesh_core::{Certainty, EvidenceKind, Privilege};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn member(id: u64, level: u64) -> Value {
    json!({"id":id,"username":format!("user-{id}"),"state":"active","access_level":level,"email":"not-verified@example.test"})
}
async fn mock<F>(handler: F) -> (GitlabProvider, tokio::task::JoinHandle<()>)
where
    F: Fn(&str) -> (u16, String, String) + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut bytes = vec![0; 8192];
            let n = stream.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(request.starts_with("GET "));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("private-token: synthetic-private-token")
            );
            let (status, headers, body) = handler(request.split_whitespace().nth(1).unwrap());
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{headers}\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    let mut provider = GitlabProvider::new(
        "gl".into(),
        "https://gitlab.example".into(),
        vec!["10".into()],
        vec![],
        Secret::new("synthetic-private-token".into()),
    )
    .unwrap();
    provider.origin = Url::parse(&format!("http://{address}/")).unwrap();
    (provider, task)
}
fn routes(target: &str) -> (u16, String, String) {
    let path = target.split('?').next().unwrap();
    let value = match path {
        "/api/v4/user" => json!({"id":1,"username":"operator","state":"active"}),
        "/api/v4/groups/10" => json!({"id":10,"full_path":"engineering","parent_id":null}),
        "/api/v4/groups/10/projects" => {
            assert!(target.contains("with_shared=false"));
            assert!(target.contains("include_subgroups=false"));
            json!([{"id":20,"namespace":{"id":10,"kind":"group"}}])
        }
        "/api/v4/projects/20" => {
            json!({"id":20,"path_with_namespace":"engineering/project","namespace":{"id":10,"kind":"group"}})
        }
        "/api/v4/groups/10/members" => json!([member(1, 30)]),
        "/api/v4/groups/10/members/all" => json!([member(1, 40), member(2, 20)]),
        "/api/v4/projects/20/members" => json!([member(3, 30)]),
        "/api/v4/projects/20/members/all" => json!([member(1, 40), member(3, 30)]),
        _ => panic!("unexpected target {target}"),
    };
    (200, String::new(), value.to_string())
}
#[tokio::test]
async fn direct_and_collapsed_membership_keep_native_scope_and_distinct_evidence() {
    let (provider, task) = mock(routes).await;
    provider.check().await.unwrap();
    let snapshot = provider.discover().await.unwrap();
    task.abort();
    snapshot.validate().unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 3);
    assert_eq!(snapshot.resources.len(), 2);
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(snapshot.memberships.len(), 1);
    assert_eq!(snapshot.grants.len(), 6);
    assert!(
        snapshot
            .accounts
            .iter()
            .all(|a| a.verified_emails.is_empty()
                && a.key.id.starts_with("gitlab:gitlab.example:443:user:"))
    );
    let project = snapshot
        .resources
        .iter()
        .find(|r| r.kind.as_deref() == Some("gitlab.project"))
        .unwrap();
    assert!(project.parent.is_some());
    let direct = snapshot
        .grants
        .iter()
        .find(|g| {
            g.role == "access_level:30:developer"
                && g.provenance.method == "gitlab.groups.members.direct"
        })
        .unwrap();
    assert_eq!(direct.evidence_kind, EvidenceKind::Assignment);
    assert_eq!(direct.certainty, Certainty::Observed);
    let effective = snapshot
        .grants
        .iter()
        .find(|g| {
            g.provenance.method == "gitlab.projects.members_all.effective_collapsed"
                && g.role == "access_level:40:maintainer"
        })
        .unwrap();
    assert_eq!(effective.evidence_kind, EvidenceKind::Permission);
    assert_eq!(effective.privilege, Privilege::Admin);
    assert!(snapshot.identities.is_empty());
}
#[tokio::test]
async fn later_pages_retain_evidence_but_permission_failures_are_partial() {
    let (provider, task) = mock(|target| {
        if target.starts_with("/api/v4/projects/20/members?") {
            if target.contains("page=2") {
                return (403, String::new(), "synthetic-private-token".into());
            }
            return (
                200,
                "X-Next-Page: 2\r\n".into(),
                json!([member(3, 30)]).to_string(),
            );
        }
        routes(target)
    })
    .await;
    let snapshot = provider.discover().await.unwrap();
    task.abort();
    assert!(!snapshot.complete);
    assert!(
        snapshot
            .grants
            .iter()
            .any(|g| g.provenance.method == "gitlab.projects.members.direct")
    );
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("synthetic-private-token")
    );
}
#[tokio::test]
async fn cross_origin_pagination_redirects_and_reflection_never_leak_credentials() {
    for case in ["link", "redirect", "reflection"] {
        let (provider,task)=mock(move |target| {
            if target.starts_with("/api/v4/groups/10/members?") {return match case {
                "link"=>(200,"Link: <https://attacker.example/api/v4/groups/10/members?per_page=100&page=2>; rel=\"next\"\r\n".into(),json!([member(9,50)]).to_string()),
                "redirect"=>(302,"Location: https://attacker.example/\r\n".into(),String::new()),
                _=>(200,String::new(),json!([{"id":9,"username":"synthetic-private-token","access_level":50}]).to_string()),
            };}
            routes(target)
        }).await;
        let snapshot = provider.discover().await.unwrap();
        task.abort();
        assert!(!snapshot.complete, "{case}");
        assert!(!snapshot.accounts.iter().any(|a| a.key.id.ends_with(":9")));
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("synthetic-private-token")
        );
    }
}
#[tokio::test]
async fn custom_unknown_roles_remain_unknown_and_conflicting_accounts_are_suppressed() {
    let (provider,task)=mock(|target| {
        if target.starts_with("/api/v4/groups/10/members?") {return (200,String::new(),json!([{"member_role_id":900,"access_level":50,"id":7,"username":"custom","state":"active"},member(8,99)]).to_string());}
        if target.starts_with("/api/v4/projects/20/members?") {let mut row=member(1,30);row["username"]=json!("contradictory");return (200,String::new(),json!([row]).to_string());}
        routes(target)
    }).await;
    let snapshot = provider.discover().await.unwrap();
    task.abort();
    assert!(!snapshot.complete);
    assert!(
        snapshot
            .grants
            .iter()
            .filter(|g| g.role.contains("member_role_id") || g.role.contains("99:unknown"))
            .all(|g| g.privilege == Privilege::Unknown)
    );
    assert!(!snapshot.accounts.iter().any(|a| a.key.id.ends_with(":1")));
    snapshot.validate().unwrap();
}
#[tokio::test]
async fn foreign_project_scope_and_duplicate_page_ids_do_not_create_false_paths() {
    for case in ["scope", "duplicate"] {
        let (provider, task) = mock(move |target| {
            if case == "scope" && target.starts_with("/api/v4/groups/10/projects?") {
                return (
                    200,
                    String::new(),
                    json!([{"id":99,"namespace":{"id":88,"kind":"group"}}]).to_string(),
                );
            }
            if case == "duplicate" && target.starts_with("/api/v4/groups/10/members?") {
                return (
                    200,
                    String::new(),
                    json!([member(8, 20), member(8, 50)]).to_string(),
                );
            }
            routes(target)
        })
        .await;
        let snapshot = provider.discover().await.unwrap();
        task.abort();
        assert!(!snapshot.complete);
        assert!(!snapshot.resources.iter().any(|r| r.key.id.ends_with(":99")));
        assert!(!snapshot.accounts.iter().any(|a| a.key.id.ends_with(":8")));
    }
}
#[tokio::test]
async fn same_scope_pagination_is_local_bounded_and_rate_limits_are_redacted() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let attempts = Arc::new(AtomicUsize::new(0));
    let seen = attempts.clone();
    let (provider, task) = mock(move |target| {
        if target.starts_with("/api/v4/groups/10/members?") {
            if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                return (
                    429,
                    "Retry-After: 0\r\n".into(),
                    "synthetic-private-token".into(),
                );
            }
            if target.contains("page=2") {
                return (
                    200,
                    "X-Page: 2\r\nX-Next-Page: \r\n".into(),
                    json!([member(8, 20)]).to_string(),
                );
            }
            return (
                200,
                "X-Page: 1\r\nX-Next-Page: 2\r\nX-Total-Pages: 2\r\n".into(),
                json!([member(7, 20)]).to_string(),
            );
        }
        routes(target)
    })
    .await;
    let snapshot = provider.discover().await.unwrap();
    task.abort();
    assert!(snapshot.complete);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert!(snapshot.accounts.iter().any(|a| a.key.id.ends_with(":8")));
    let (provider, task) = mock(|target| {
        if target.starts_with("/api/v4/groups/10/members?") {
            (
                429,
                "Retry-After: 999\r\n".into(),
                "synthetic-private-token".into(),
            )
        } else {
            routes(target)
        }
    })
    .await;
    let snapshot = provider.discover().await.unwrap();
    task.abort();
    assert!(!snapshot.complete);
    assert!(
        snapshot
            .limitations
            .iter()
            .any(|v| v.contains("rate_limit"))
    );
    assert!(!format!("{snapshot:?}").contains("synthetic-private-token"));
}
#[tokio::test]
async fn deadlines_and_network_configuration_fail_closed() {
    let (mut provider, task) = mock(|_| (200, String::new(), "{}".into())).await;
    provider.operation_timeout = Duration::ZERO;
    assert!(provider.discover().await.is_err());
    task.abort();
    let network = permesh_provider_sdk::network::NetworkContext {
        https_proxy: Some("http://user:password@proxy.example".into()),
        no_proxy: vec![],
        ca_bundle_pem: None,
    };
    assert!(
        GitlabProvider::new_with_network(
            "gl".into(),
            "https://gitlab.example".into(),
            vec!["10".into()],
            vec![],
            Secret::new("synthetic-private-token".into()),
            Some(&network)
        )
        .is_err()
    );
    assert!(!valid_configuration("https://gitlab.example", &[], &[]));
    assert!(!valid_configuration(
        "https://gitlab.example",
        &["01".into()],
        &[]
    ));
}
#[tokio::test]
async fn runtime_http_exchange_preserves_negotiation_and_hides_credentials() {
    use permesh_native_runtime::Adapter;
    let (provider, task) = mock(routes).await;
    let input=json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"gl","operation":"discover","features":["network_v1"]}).to_string()+"\n"+
        &json!({"protocol_version":1,"id":"discover","method":"discover","configuration":{"origin":"https://gitlab.example","group_ids":["10"]},"credentials":{"token":"synthetic-private-token"},"network":{"https_proxy":"http://127.0.0.1:1234","no_proxy":["127.0.0.1"]}}).to_string()+"\n";
    let (mut sender, reader) = tokio::io::duplex(8192);
    sender.write_all(input.as_bytes()).await.unwrap();
    let mut output = Vec::new();
    permesh_native_runtime::serve_with_network::<protocol::Gitlab, _, _, _, _, _>(
        reader,
        &mut output,
        |instance, config, credentials, network| async move {
            assert_eq!(instance, "gl");
            assert_eq!(config.group_ids, vec!["10"]);
            assert_eq!(credentials.token.as_str(), "synthetic-private-token");
            assert!(network.is_some());
            Ok(provider)
        },
    )
    .await
    .unwrap();
    drop(sender);
    task.abort();
    assert!(!String::from_utf8_lossy(&output).contains("synthetic-private-token"));
    let metadata = provider_metadata();
    let mut decoder =
        permesh_provider_protocol::negotiated::DiscoveryDecoder::with_required_features(
            "gitlab",
            "gl",
            Some(&metadata.capabilities),
            &[permesh_provider_protocol::negotiated::Feature::NetworkV1],
        )
        .unwrap();
    for frame in output.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    assert_eq!(decoder.finish().unwrap().grants.len(), 6);
    protocol::Gitlab::setup().validate().unwrap();
}
