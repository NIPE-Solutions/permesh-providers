// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(clippy::unwrap_used)]
use super::*;
use permesh_core::{Certainty, Privilege};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const ACCOUNT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MEMBER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const GROUP: &str = "cccccccccccccccccccccccccccccccc";
const ZONE: &str = "dddddddddddddddddddddddddddddddd";
const POLICY: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const ROLE: &str = "ffffffffffffffffffffffffffffffff";
fn envelope(result: Value) -> String {
    json!({"success":true,"result":result}).to_string()
}
fn policy(access: &str, target: &str) -> Value {
    json!({"id":POLICY,"access":access,"permission_groups":[{"id":ROLE,"name":"Custom Manager"}],"resource_groups":[{"id":GROUP,"scope":{"key":format!("com.cloudflare.api.account.{ACCOUNT}"),"objects":[{"key":target}]}}]})
}
fn member(policies: Value) -> Value {
    json!({"id":MEMBER,"status":"accepted","user":{"id":ROLE,"email":"alice@example.com"},"policies":policies})
}
async fn mock<F>(handler: F) -> (CloudflareProvider, tokio::task::JoinHandle<()>)
where
    F: Fn(&str) -> (u16, String, String) + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut bytes = vec![0; 8192];
            let n = stream.read(&mut bytes).await.unwrap();
            let req = String::from_utf8_lossy(&bytes[..n]);
            assert!(req.starts_with("GET "));
            assert!(
                req.to_ascii_lowercase()
                    .contains("authorization: bearer test-secret")
            );
            let (status, headers, body) = handler(req.split_whitespace().nth(1).unwrap());
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{headers}\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    let mut provider = CloudflareProvider::new(
        "cf".into(),
        ACCOUNT.into(),
        Secret::new("test-secret".into()),
    )
    .unwrap();
    provider.origin = Url::parse(&format!("http://{address}/client/v4/")).unwrap();
    (provider, task)
}
fn routes(
    target: &str,
    members: Value,
    groups: Value,
    group_members: Value,
) -> (u16, String, String) {
    let path = target.split('?').next().unwrap();
    let result = if path == format!("/client/v4/accounts/{ACCOUNT}") {
        json!({"id":ACCOUNT,"name":"Example"})
    } else if path.ends_with(&format!("/iam/user_groups/{GROUP}/members")) {
        group_members
    } else if path.ends_with("/iam/user_groups") {
        groups
    } else if path.ends_with("/members") {
        members
    } else if path.ends_with("/zones") {
        json!([{"id":ZONE,"name":"example.com","account":{"id":ACCOUNT}}])
    } else {
        panic!("unexpected target {target}")
    };
    (200, String::new(), envelope(result))
}
#[tokio::test]
async fn observes_members_groups_and_exact_zone_assignments_without_verified_email() {
    let (p, t) = mock(|target| {
        routes(
            target,
            json!([member(json!([policy(
                "allow",
                &format!("com.cloudflare.api.account.zone.{ZONE}")
            )]))]),
            json!([{"id":GROUP,"name":"Operations","policies":[policy("allow","*")]}]),
            json!([{"id":MEMBER,"status":"accepted"}]),
        )
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(s.complete);
    assert_eq!(s.accounts.len(), 1);
    assert!(s.identities.is_empty());
    assert_eq!(s.accounts[0].key.id, format!("member:{ACCOUNT}:{MEMBER}"));
    assert!(s.accounts[0].verified_emails.is_empty());
    assert_eq!(s.groups.len(), 1);
    assert_eq!(s.memberships.len(), 1);
    assert_eq!(s.grants.len(), 2);
    assert!(
        s.grants
            .iter()
            .all(|g| matches!(g.privilege, Privilege::Unknown)
                && matches!(g.certainty, Certainty::Observed))
    );
    assert!(
        s.grants
            .iter()
            .any(|g| g.resource.id == format!("zone:{ZONE}"))
    );
    assert!(
        !s.grants
            .iter()
            .any(|g| g.resource.id == format!("account:{ACCOUNT}"))
    );
    t.abort();
}
#[tokio::test]
async fn deny_and_unknown_scope_never_become_positive_access() {
    for policies in [
        json!([policy("allow", "*"), policy("deny", "*")]),
        json!([policy("allow", "unsupported:resource")]),
    ] {
        let (p, t) = mock(move |target| {
            routes(
                target,
                json!([member(policies.clone())]),
                json!([]),
                json!([]),
            )
        })
        .await;
        let s = p.discover().await.unwrap();
        s.validate().unwrap();
        assert!(!s.complete);
        assert!(s.grants.is_empty());
        t.abort();
    }
}
#[tokio::test]
async fn pending_member_has_no_grants_or_group_paths() {
    let (p, t) = mock(|target| {
        let mut m = member(json!([policy("allow", "*")]));
        m["status"] = json!("pending");
        routes(
            target,
            json!([m]),
            json!([{"id":GROUP,"name":"Operations","policies":[policy("allow","*")]}]),
            json!([{"id":MEMBER,"status":"pending"}]),
        )
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(s.accounts.is_empty());
    assert!(s.memberships.is_empty());
    t.abort();
}
#[tokio::test]
async fn cross_account_zones_and_conflicting_members_are_excluded() {
    let (p, t) = mock(|target| {
        if target.contains("/zones?") {
            return (
                200,
                String::new(),
                envelope(json!([{"id":ZONE,"name":"foreign","account":{"id":ROLE}}])),
            );
        }
        let mut conflict = member(json!([]));
        conflict["status"] = json!("pending");
        routes(
            target,
            json!([member(json!([policy("allow", "*")])), conflict]),
            json!([]),
            json!([]),
        )
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(!s.complete);
    assert!(s.accounts.is_empty());
    assert!(s.grants.is_empty());
    assert!(
        !s.resources
            .iter()
            .any(|r| r.key.id == format!("zone:{ZONE}"))
    );
    t.abort();
}
#[tokio::test]
async fn authorization_failures_are_sanitized_and_partial_endpoint_failures_remain_visible() {
    let (p, t) = mock(|_| (401, String::new(), "test-secret".into())).await;
    let e = p.discover().await.unwrap_err();
    assert!(!e.to_string().contains("test-secret"));
    assert!(p.check().await.is_err());
    t.abort();
    let (p, t) = mock(|target| {
        if target.ends_with("/iam/user_groups?per_page=50&page=1") {
            (403, String::new(), "test-secret".into())
        } else {
            routes(target, json!([member(json!([]))]), json!([]), json!([]))
        }
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(!s.complete);
    assert_eq!(s.accounts.len(), 1);
    assert!(!s.limitations.join(" ").contains("test-secret"));
    t.abort();
}
#[tokio::test]
async fn changing_pagination_totals_cannot_claim_complete_collection() {
    let (p,t)=mock(|target| {
 if target.contains(&format!("accounts/{ACCOUNT}/members?")) {
 let (rows,total)=if target.ends_with("page=1"){(json!([member(json!([]))]),10)}else{let mut m=member(json!([]));m["id"]=json!(ROLE);(json!([m]),2)};
 return (200,String::new(),json!({"success":true,"result":rows,"result_info":{"page":if target.ends_with("page=1"){1}else{2},"per_page":50,"count":1,"total_count":total}}).to_string());
 }routes(target,json!([]),json!([]),json!([]))}).await;
    let s = p.discover().await.unwrap();
    assert!(!s.complete);
    assert!(s.limitations.iter().any(|s| s.contains("pagination")));
    t.abort();
}
#[tokio::test]
async fn new_policy_conditions_and_conflicting_role_definitions_are_not_ignored() {
    let mut conditional = policy("allow", "*");
    conditional["condition"] = json!({"country":"AT"});
    let mut roles = policy("allow", "*");
    roles["permission_groups"] =
        json!([{"id":ROLE,"name":"Reader"},{"id":ROLE,"name":"Administrator"}]);
    for policy in [conditional, roles] {
        let (p, t) = mock(move |target| {
            routes(
                target,
                json!([member(json!([policy]))]),
                json!([]),
                json!([]),
            )
        })
        .await;
        let s = p.discover().await.unwrap();
        assert!(!s.complete);
        assert!(s.grants.is_empty());
        t.abort();
    }
}
#[tokio::test]
async fn group_deny_suppresses_other_access_paths_for_its_members() {
    let (p, t) = mock(|target| {
        routes(
            target,
            json!([member(json!([policy("allow", "*")]))]),
            json!([{"id":GROUP,"name":"Restricted","policies":[policy("deny","*")]}]),
            json!([{"id":MEMBER,"status":"accepted"}]),
        )
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(!s.complete);
    assert!(s.grants.is_empty());
    assert!(s.memberships.is_empty());
    t.abort();
}
#[tokio::test]
async fn member_pagination_preserves_successful_pages_on_next_page_failure() {
    let (p,t)=mock(|target|{
 if target.contains(&format!("accounts/{ACCOUNT}/members?")) {
 if target.ends_with("page=2"){return (403,String::new(),"test-secret".into())}
 return (200,String::new(),json!({"success":true,"result":[member(json!([]))],"result_info":{"page":1,"per_page":50,"count":1,"total_count":2}}).to_string());
 }routes(target,json!([]),json!([]),json!([]))}).await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(!s.complete);
    assert_eq!(s.accounts.len(), 1);
    t.abort();
}
#[tokio::test]
async fn request_retries_transient_responses_but_never_follows_redirects() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let attempts = Arc::new(AtomicUsize::new(0));
    let copy = attempts.clone();
    let (p, t) = mock(move |target| {
        if copy.fetch_add(1, Ordering::SeqCst) == 0 {
            (503, "Retry-After: 0\r\n".into(), "test-secret".into())
        } else {
            routes(target, json!([]), json!([]), json!([]))
        }
    })
    .await;
    assert!(p.check().await.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    t.abort();
    let (p, t) = mock(|_| {
        (
            302,
            "Location: https://example.invalid/collect\r\n".into(),
            "".into(),
        )
    })
    .await;
    assert!(p.check().await.is_err());
    t.abort();
}
#[tokio::test]
async fn success_false_oversized_and_long_retry_responses_fail_safely() {
    for (status, headers, body) in [
        (
            200,
            "".to_string(),
            json!({"success":false,"errors":[{"message":"test-secret"}]}).to_string(),
        ),
        (200, "".into(), "x".repeat(MAX_BODY + 1)),
        (429, "Retry-After: 300\r\n".into(), "test-secret".into()),
    ] {
        let (p, t) = mock(move |_| (status, headers.clone(), body.clone())).await;
        let e = p.check().await.unwrap_err();
        assert!(!e.to_string().contains("test-secret"));
        t.abort();
    }
}
#[tokio::test]
async fn timeout_covers_slow_response_body_and_cancellation_drops_collection() {
    use std::time::Duration;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut data = [0; 8192];
                let _ = socket.read(&mut data).await;
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
                    .await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            });
        }
    });
    let mut p = CloudflareProvider::new(
        "cf".into(),
        ACCOUNT.into(),
        Secret::new("test-secret".into()),
    )
    .unwrap();
    p.origin = Url::parse(&format!("http://{address}/client/v4/")).unwrap();
    p.request_timeout = Duration::from_millis(30);
    assert_eq!(p.check().await.unwrap_err().code, "timeout");
    assert!(
        tokio::time::timeout(Duration::from_millis(10), p.discover())
            .await
            .is_err()
    );
    task.abort();
}
#[tokio::test]
async fn total_assignment_expansion_is_bounded_across_members() {
    let (p, t) = mock(|target| {
        if target.contains(&format!("accounts/{ACCOUNT}/members?")) {
            let page = target
                .rsplit("page=")
                .next()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            if page > 3 {
                return (200, String::new(), envelope(json!([])));
            }
            let roles: Vec<_> = (1..=150)
                .map(|i| json!({"id":format!("{i:032x}"),"name":"reader"}))
                .collect();
            let mut pol = policy("allow", "*");
            pol["permission_groups"] = json!(roles);
            let members: Vec<_> = (0..50)
                .map(|i| {
                    let mut m = member(json!([pol]));
                    m["id"] = json!(format!("{:032x}", page * 50 + i));
                    m
                })
                .collect();
            return (200, String::new(), envelope(json!(members)));
        }
        routes(target, json!([]), json!([]), json!([]))
    })
    .await;
    let result = p.discover().await;
    assert!(matches!(result,Err(e) if e.code=="limit"));
    t.abort();
}
#[tokio::test]
async fn future_scope_constraints_cannot_be_silently_discarded() {
    let mut pol = policy("allow", "*");
    pol["resource_groups"][0]["scope"]["condition"] = json!({"tag":"production"});
    let (p, t) =
        mock(move |target| routes(target, json!([member(json!([pol]))]), json!([]), json!([])))
            .await;
    let s = p.discover().await.unwrap();
    assert!(!s.complete);
    assert!(s.grants.is_empty());
    t.abort();
}
#[tokio::test]
async fn shared_runtime_discovery_cross_decodes_assignments_and_partial_categories() {
    use permesh_provider_protocol::DiscoveryDecoder;
    for status in [200, 403, 401] {
        let partial = status != 200;
        let (p, t) = mock(move |target| {
            if partial && target.contains("/iam/user_groups?") {
                (status, String::new(), "test-secret".into())
            } else {
                routes(
                    target,
                    json!([member(json!([policy(
                        "allow",
                        &format!("com.cloudflare.api.account.zone.{ZONE}")
                    )]))]),
                    json!([]),
                    json!([]),
                )
            }
        })
        .await;
        let origin = p.origin.clone();
        let (mut host, peer) = tokio::io::duplex(128 * 1024);
        let (reader, writer) = tokio::io::split(peer);
        let task = tokio::spawn(async move {
            permesh_native_runtime::serve::<crate::protocol::Cloudflare, _, _, _, _, _>(
                reader,
                writer,
                move |id, config, mut credentials| async move {
                    let mut p = CloudflareProvider::new(
                        id,
                        config.account_id,
                        Secret::new(std::mem::take(&mut *credentials.token)),
                    )?;
                    p.origin = origin;
                    Ok(p)
                },
            )
            .await
        });
        let request = format!(
            "{}\n{}\n",
            json!({"protocol":2,"id":"handshake","method":"handshake","instance":"cf"}),
            json!({"protocol":2,"id":"discover","method":"discover","configuration":{"account_id":ACCOUNT},"credentials":{"token":"test-secret"}})
        );
        host.write_all(request.as_bytes()).await.unwrap();
        let mut output = Vec::new();
        host.read_to_end(&mut output).await.unwrap();
        assert!(task.await.unwrap().is_ok());
        assert!(!String::from_utf8_lossy(&output).contains("test-secret"));
        let mut decoder = DiscoveryDecoder::new_versioned(
            "cloudflare",
            "cf",
            Some(&provider_metadata().capabilities),
            2,
        )
        .unwrap();
        for frame in output.split_inclusive(|b| *b == b'\n') {
            decoder.push_frame(frame).unwrap();
        }
        let snapshot = decoder.finish().unwrap();
        snapshot.validate().unwrap();
        assert_eq!(snapshot.complete, !partial);
        assert_eq!(snapshot.accounts.len(), 1);
        assert_eq!(snapshot.grants.len(), 1);
        assert!(matches!(snapshot.grants[0].privilege, Privilege::Unknown));
        t.abort();
    }
}
#[test]
fn constructors_reject_unsafe_credential_and_scope_inputs() {
    for token in ["", "line\nbreak", "token with spaces", "unicode-ä"] {
        assert!(
            CloudflareProvider::new("cf".into(), ACCOUNT.into(), Secret::new(token.into()))
                .is_err()
        );
    }
    for account in ["", "../accounts", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"] {
        assert!(
            CloudflareProvider::new("cf".into(), account.into(), Secret::new("token".into()))
                .is_err()
        );
    }
}
