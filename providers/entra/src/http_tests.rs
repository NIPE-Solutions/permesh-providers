// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use crate::tests::*;
use crate::*;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
fn next_link(req: &str) -> String {
    let host = req
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let target = req.split_whitespace().nth(1).unwrap();
    format!("http://{host}{target}&%24skiptoken=next")
}
#[tokio::test]
async fn follows_valid_opaque_next_link_and_keeps_valid_pages_on_denial() {
    let (p, t) = mock(false, |req| {
        let target = req.split_whitespace().nth(1).unwrap();
        if target.starts_with("/v1.0/users?") {
            if target.contains("skiptoken") {
                return (403, String::new(), "REMOTE_SECRET".into());
            }
            return (
                200,
                String::new(),
                json!({"value":[{"id":USER,"userType":"Member"}],"@odata.nextLink":next_link(req)})
                    .to_string(),
            );
        }
        routes(req)
    })
    .await;
    let snapshot = p.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(
        snapshot.accounts[0].status,
        permesh_core::IdentityStatus::Unknown
    );
    assert_eq!(
        snapshot.accounts[0].affiliation,
        permesh_core::Affiliation::Unknown
    );
    assert_eq!(snapshot.memberships.len(), 1);
    assert!(!format!("{snapshot:?}").contains("REMOTE_SECRET"));
    assert!(snapshot.limitations.iter().any(|s| s.contains("forbidden")));
    t.abort();
}
#[tokio::test]
async fn throttling_obeys_bounded_retry_after_and_missing_retry_header_is_partial() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let (p, t) = mock(false, move |req| {
        if req.contains("/v1.0/users?") && seen.fetch_add(1, Ordering::SeqCst) == 0 {
            return (429, "Retry-After: 0\r\n".into(), "REMOTE_SECRET".into());
        }
        routes(req)
    })
    .await;
    assert!(p.discover().await.unwrap().complete);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    t.abort();
    let (p, t) = mock(false, |req| {
        if req.contains("/v1.0/groups?") {
            return (429, String::new(), "REMOTE_SECRET".into());
        }
        routes(req)
    })
    .await;
    let snapshot = p.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 1);
    assert!(
        snapshot
            .limitations
            .iter()
            .any(|s| s.contains("rate_limit"))
    );
    t.abort();
}
#[tokio::test]
async fn foreign_redirect_pagination_and_duplicate_fields_never_expand_scope() {
    for mode in 0..3 {
        let(p,t)=mock(false,move|req|{if req.contains("/v1.0/users?"){return match mode{0=>(200,String::new(),json!({"value":[{"id":USER}],"@odata.nextLink":"https://evil.invalid/v1.0/users?$skiptoken=secret"}).to_string()),1=>(302,"Location: https://evil.invalid/stolen\r\n".into(),String::new()),_=>(200,String::new(),format!(r#"{{"value":[{{"id":"{USER}","id":"{SERVICE}"}}]}}"#))};}routes(req)}).await;
        let snapshot = p.discover().await.unwrap();
        assert!(!snapshot.complete);
        if mode == 0 {
            assert_eq!(snapshot.accounts.len(), 1);
        } else {
            assert!(snapshot.accounts.is_empty());
        }
        t.abort();
    }
}
#[tokio::test]
async fn known_omission_is_qualified_and_unknown_members_are_partial() {
    let(p,t)=mock(true,|req|{if req.contains(&format!("/groups/{GROUP}/members")){return value(json!([{"id":USER,"@odata.type":"#microsoft.graph.user"},{"id":SERVICE,"@odata.type":"#microsoft.graph.servicePrincipal"}]));}routes(req)}).await;
    let snapshot = p.discover().await.unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 2);
    assert_eq!(snapshot.memberships.len(), 1);
    t.abort();
    for kind in [
        "#microsoft.graph.device",
        "#microsoft.graph.orgContact",
        "#microsoft.graph.futureType",
    ] {
        let (p, t) = mock(false, move |req| {
            if req.contains(&format!("/groups/{GROUP}/members")) {
                return value(json!([{"id":SERVICE,"@odata.type":kind}]));
            }
            routes(req)
        })
        .await;
        let snapshot = p.discover().await.unwrap();
        assert!(!snapshot.complete);
        assert!(snapshot.memberships.is_empty());
        assert_eq!(snapshot.accounts.len(), 1);
        t.abort();
    }
}
#[tokio::test]
async fn canonical_authority_needs_explicit_native_account_mapping_without_verified_upn() {
    let (p, t) = mock(false, routes).await;
    let snapshot = p.discover().await.unwrap();
    let canonical = &snapshot.identities[0].id;
    let unbound = permesh_core::query_user(
        std::slice::from_ref(&snapshot),
        &Default::default(),
        canonical,
    )
    .unwrap();
    assert!(unbound.accounts.is_empty());
    let mut aliases = permesh_core::Aliases::new();
    aliases
        .entry(canonical.clone())
        .or_default()
        .entry("entra".into())
        .or_default()
        .push(snapshot.accounts[0].key.id.clone());
    let bound =
        permesh_core::query_user(std::slice::from_ref(&snapshot), &aliases, canonical).unwrap();
    assert_eq!(bound.accounts.len(), 1);
    assert!(bound.accounts[0].verified_emails.is_empty());
    t.abort();
}
#[tokio::test]
async fn unknown_workload_type_and_restricted_attributes_stay_unknown() {
    let (p, t) = mock(true, |req| {
        if req.contains("/servicePrincipals?") {
            return value(
                json!([{"id":SERVICE,"servicePrincipalType":"FutureType","accountEnabled":null}]),
            );
        }
        routes(req)
    })
    .await;
    let snapshot = p.discover().await.unwrap();
    let account = snapshot
        .accounts
        .iter()
        .find(|a| a.key.id.ends_with(SERVICE))
        .unwrap();
    assert_eq!(account.kind, permesh_core::IdentityKind::Unknown);
    assert_eq!(account.status, permesh_core::IdentityStatus::Unknown);
    t.abort();
}

#[tokio::test]
async fn negotiated_host_decoder_roundtrips_directory_dimensions_and_partial_status() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for denied in [false, true] {
        let (p, server) = mock(false, move |req| {
            if denied && req.contains("/v1.0/groups?") {
                return (403, String::new(), "SYNTHETIC_TOKEN".into());
            }
            routes(req)
        })
        .await;
        let (mut host, peer) = tokio::io::duplex(128 * 1024);
        let (reader, writer) = tokio::io::split(peer);
        let task = tokio::spawn(async move {
            permesh_native_runtime::serve_with_network::<crate::protocol::Entra, _, _, _, _, _>(
                reader,
                writer,
                move |_, _, _, _| async move { Ok(p) },
            )
            .await
        });
        let request = format!(
            "{}\n{}\n",
            json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"entra","operation":"discover"}),
            json!({"protocol_version":1,"id":"discover","method":"discover","configuration":{"tenant_id":TENANT},"credentials":{"token":"SYNTHETIC_TOKEN"}})
        );
        host.write_all(request.as_bytes()).await.unwrap();
        let mut output = vec![];
        host.read_to_end(&mut output).await.unwrap();
        assert!(task.await.unwrap().is_ok());
        assert!(!String::from_utf8_lossy(&output).contains("SYNTHETIC_TOKEN"));
        let mut decoder = permesh_provider_protocol::negotiated::DiscoveryDecoder::new(
            "entra",
            "entra",
            Some(&crate::provider_metadata().capabilities),
        )
        .unwrap();
        for frame in output.split_inclusive(|b| *b == b'\n') {
            decoder.push_frame(frame).unwrap();
        }
        let snapshot = decoder.finish().unwrap();
        assert_eq!(snapshot.complete, !denied);
        assert_eq!(
            snapshot.identities[0].status,
            permesh_core::IdentityStatus::Inactive
        );
        assert_eq!(
            snapshot.accounts[0].kind,
            permesh_core::IdentityKind::Unknown
        );
        assert!(snapshot.accounts[0].verified_emails.is_empty());
        assert!(snapshot.grants.is_empty());
        server.abort();
    }
}
#[tokio::test]
async fn pagination_rejects_query_changes_userinfo_and_repeat_pages() {
    let (p, t) = mock(false, routes).await;
    let initial = p.url("users", Some("id,displayName")).unwrap();
    for next in [
        "https://graph.microsoft.com/v1.0/users?$skiptoken=x",
        "http://user@127.0.0.1/v1.0/users?$skiptoken=x",
    ] {
        assert!(p.next_url(&initial, next).is_err());
    }
    let mut next = initial.clone();
    next.query_pairs_mut()
        .append_pair("$skiptoken", "opaque")
        .append_pair("$filter", "accountEnabled eq true");
    assert!(p.next_url(&initial, next.as_str()).is_err());
    next = initial.clone();
    next.query_pairs_mut().append_pair("$skiptoken", "opaque");
    assert!(p.next_url(&initial, next.as_str()).is_ok());
    t.abort();
    let (p, t) = mock(false, |req| {
        if req.contains("/v1.0/users?") {
            return (
                200,
                String::new(),
                json!({"value":[{"id":USER}],"@odata.nextLink":next_link(req)}).to_string(),
            );
        }
        routes(req)
    })
    .await;
    let snapshot = p.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 1);
    t.abort();
}

#[tokio::test]
async fn nested_group_edges_are_direct_and_do_not_create_authorization() {
    let (p, t) = mock(false, |req| {
        let path = req
            .split_whitespace()
            .nth(1)
            .unwrap()
            .split('?')
            .next()
            .unwrap();
        if path == "/v1.0/groups" {
            return value(json!([{"id":GROUP},{"id":SERVICE}]));
        }
        if path == format!("/v1.0/groups/{GROUP}/members") {
            return value(json!([{"id":SERVICE,"@odata.type":"#microsoft.graph.group"}]));
        }
        if path == format!("/v1.0/groups/{SERVICE}/members") {
            return value(json!([{"id":USER,"@odata.type":"#microsoft.graph.user"}]));
        }
        routes(req)
    })
    .await;
    let snapshot = p.discover().await.unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.groups.len(), 2);
    assert_eq!(snapshot.memberships.len(), 2);
    assert!(
        snapshot
            .memberships
            .iter()
            .any(|m| matches!(m.member, permesh_core::Subject::Group(_)))
    );
    assert!(snapshot.grants.is_empty());
    t.abort();
}
#[tokio::test]
async fn body_page_and_retry_budgets_preserve_earlier_observations() {
    for mode in 0..3 {
        let(p,t)=mock(false,move|req|{if req.contains("/v1.0/groups?"){return match mode{0=>(200,String::new(),"x".repeat(crate::MAX_BODY+1)),1=>(429,"Retry-After: 3600\r\n".into(),"REMOTE_SECRET".into()),_=>(200,String::new(),json!({"value":(0..1000).map(|n|json!({"id":format!("00000000-0000-0000-0000-{n:012x}")})).collect::<Vec<_>>()}).to_string())};}routes(req)}).await;
        let snapshot = p.discover().await.unwrap();
        assert!(!snapshot.complete);
        assert_eq!(snapshot.accounts.len(), 1);
        assert!(snapshot.groups.is_empty());
        t.abort();
    }
}
#[test]
fn unsafe_configuration_and_private_key_ca_are_rejected_before_request() {
    for tenant in [
        "common",
        "../../organization",
        "11111111-1111-1111-1111-11111111111z",
    ] {
        assert!(
            EntraProvider::new(
                "entra".into(),
                tenant.into(),
                false,
                permesh_secrets::Secret::new("token".into())
            )
            .is_err()
        );
    }
    let network = permesh_provider_sdk::network::NetworkContext {
        ca_bundle_pem: Some(
            "-----BEGIN PRIVATE KEY-----\nYQ==\n-----END PRIVATE KEY-----\n".into(),
        ),
        ..Default::default()
    };
    assert!(
        EntraProvider::new_with_network(
            "entra".into(),
            TENANT.into(),
            false,
            permesh_secrets::Secret::new("token".into()),
            Some(&network)
        )
        .is_err()
    );
}

#[tokio::test]
async fn escaped_credential_reflections_in_successful_metadata_are_rejected() {
    for mode in 0..3 {
        let (p, t) = mock(false, move |req| {
            if req.contains("/v1.0/users?") {
                let body = match mode {
                    0 => json!({"value":[{"id":USER,"displayName":"SYNTHETIC_TOKEN"}]}).to_string(),
                    1 => json!({"value":[{"id":USER,"userPrincipalName":"SYNTHETIC_TOKEN"}]})
                        .to_string()
                        .replace("SYNTHETIC_TOKEN", "\\u0053YNTHETIC_TOKEN"),
                    _ => json!({"value":[{"id":USER,"SYNTHETIC_TOKEN":"ignored"}]}).to_string(),
                };
                return (200, String::new(), body);
            }
            routes(req)
        })
        .await;
        let snapshot = p.discover().await.unwrap();
        assert!(!snapshot.complete);
        assert!(snapshot.accounts.is_empty());
        assert!(!format!("{snapshot:?}").contains("SYNTHETIC_TOKEN"));
        t.abort();
    }
}
