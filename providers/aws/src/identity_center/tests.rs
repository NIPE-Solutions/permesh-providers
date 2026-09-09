// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use super::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const CALLER: &str = "111111111111";
const ACCOUNT: &str = "222222222222";
const STORE: &str = "d-1234567890";
const INSTANCE: &str = "arn:aws:sso:::instance/ssoins-1234567890123456";
const PERMISSION: &str = "arn:aws:sso:::permissionSet/ssoins-1234567890123456/ps-1234567890123456";
const USER: &str = "11111111-1111-1111-1111-111111111111";
const GROUP: &str = "22222222-2222-2222-2222-222222222222";
fn config() -> Configuration {
    Configuration {
        account_id: CALLER.into(),
        region: "eu-west-1".into(),
        instance_arn: INSTANCE.into(),
        identity_store_id: STORE.into(),
        accounts: vec![ACCOUNT.into()],
        include_organizations: false,
        caller_role: None,
    }
}
fn credentials() -> Credentials {
    Credentials::new(
        "ASIATEST1234567890123",
        "SYNTHETIC_SECRET",
        Some("SYNTHETIC_SESSION".into()),
        None,
        "test",
    )
}
fn response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
fn route(req: &str) -> String {
    if req.contains("Action=GetCallerIdentity") {
        return response(&format!(
            "<GetCallerIdentityResponse><GetCallerIdentityResult><Account>{CALLER}</Account><Arn>arn:aws:sts::{CALLER}:assumed-role/Review/test</Arn><UserId>test</UserId></GetCallerIdentityResult></GetCallerIdentityResponse>"
        ));
    }
    let operation = req
        .lines()
        .find_map(|s| s.strip_prefix("x-amz-target: "))
        .unwrap()
        .split('.')
        .next_back()
        .unwrap();
    let value = match operation {
        "ListInstances" => {
            json!({"Instances":[{"InstanceArn":INSTANCE,"IdentityStoreId":STORE,"OwnerAccountId":CALLER}]})
        }
        "ListUsers" => {
            json!({"Users":[{"IdentityStoreId":STORE,"UserId":USER,"UserName":"review-user","UserStatus":"DISABLED"}]})
        }
        "ListGroups" => {
            json!({"Groups":[{"IdentityStoreId":STORE,"GroupId":GROUP,"DisplayName":"Reviewers"}]})
        }
        "ListGroupMemberships" => {
            json!({"GroupMemberships":[{"IdentityStoreId":STORE,"MembershipId":"33333333-3333-3333-3333-333333333333","GroupId":GROUP,"MemberId":{"UserId":USER}}]})
        }
        "ListPermissionSetsProvisionedToAccount" => json!({"PermissionSets":[PERMISSION]}),
        "DescribePermissionSet" => {
            json!({"PermissionSet":{"PermissionSetArn":PERMISSION,"Name":"AdministratorAccess"}})
        }
        "ListAccountAssignments" => {
            json!({"AccountAssignments":[{"AccountId":ACCOUNT,"PermissionSetArn":PERMISSION,"PrincipalId":GROUP,"PrincipalType":"GROUP"},{"AccountId":ACCOUNT,"PermissionSetArn":PERMISSION,"PrincipalId":USER,"PrincipalType":"USER"}]})
        }
        "ListAccounts" => {
            json!({"Accounts":[{"Id":ACCOUNT,"Name":"Reviewed account","Arn":format!("arn:aws:organizations::{CALLER}:account/o-example/{ACCOUNT}")}]})
        }
        _ => panic!("unexpected operation {operation}"),
    };
    response(&value.to_string())
}
async fn mock<F: Fn(&str) -> String + Send + Sync + 'static>(
    c: Configuration,
    f: F,
) -> (IdentityCenterProvider, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 32768];
                let n = stream.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                let req = String::from_utf8_lossy(&bytes);
                if let Some((h, b)) = req.split_once("\r\n\r\n") {
                    let size = h
                        .lines()
                        .find_map(|s| s.strip_prefix("content-length: "))
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    if b.len() >= size {
                        break;
                    }
                }
            }
            let req = String::from_utf8_lossy(&bytes);
            assert!(req.contains("AWS4-HMAC-SHA256"));
            let scope = if req.contains("Action=GetCallerIdentity") {
                "eu-west-1/sts/aws4_request"
            } else if req.contains("ListAccounts") {
                "us-east-1/organizations/aws4_request"
            } else if req.contains("ListUsers")
                || req.contains("ListGroups")
                || req.contains("ListGroupMemberships")
            {
                "eu-west-1/identitystore/aws4_request"
            } else {
                "eu-west-1/sso/aws4_request"
            };
            assert!(req.contains(scope), "unexpected signing service/region");
            assert!(req.contains("SYNTHETIC_SESSION"));
            stream.write_all(f(&req).as_bytes()).await.unwrap();
        }
    });
    (
        IdentityCenterProvider::build(
            "center".into(),
            c,
            credentials(),
            [
                endpoint.clone(),
                endpoint.clone(),
                endpoint.clone(),
                endpoint,
            ],
            None,
        )
        .unwrap(),
        task,
    )
}
#[tokio::test]
async fn scoped_native_assignments_remain_unknown_privilege() {
    let (p, t) = mock(config(), route).await;
    let s = p.collect().await.unwrap();
    assert!(s.complete);
    assert_eq!(s.accounts.len(), 1);
    assert_eq!(s.groups.len(), 1);
    assert_eq!(s.memberships.len(), 1);
    assert_eq!(s.grants.len(), 2);
    assert!(s.identities.is_empty());
    assert!(s.accounts[0].verified_emails.is_empty());
    assert_eq!(s.accounts[0].status, permesh_core::IdentityStatus::Inactive);
    assert!(s.grants.iter().all(|g|g.privilege==permesh_core::Privilege::Unknown && g.role.contains(PERMISSION)));
    t.abort();
}
#[tokio::test]
async fn caller_and_instance_mismatch_prevent_directory_reads() {
    for mode in 0..3 {
        let mut c = config();
        if mode == 2 {
            c.caller_role = Some("Other".into());
        }
        let (p, t) = mock(c, move |req| {
            assert!(!req.contains("ListUsers"));
            let response = route(req);
            if mode == 0 && req.contains("GetCallerIdentity") {
                response.replace(CALLER, "999999999999")
            } else if mode == 1 && req.contains("ListInstances") {
                response.replace(STORE, "d-9999999999")
            } else {
                response
            }
        })
        .await;
        assert_eq!(p.collect().await.unwrap_err().code, "scope");
        t.abort();
    }
}
#[tokio::test]
async fn later_page_denial_retains_known_users_and_marks_partial() {
    let(p,t)=mock(config(),|req|{if req.contains("ListUsers"){if req.contains("NextToken"){return "HTTP/1.1 400 Error\r\nContent-Length: 61\r\nConnection: close\r\n\r\n{\"__type\":\"AccessDeniedException\",\"message\":\"denied by test\"}".replace("Content-Length: 61",&format!("Content-Length: {}",r#"{"__type":"AccessDeniedException","message":"denied by test"}"#.len()));}return response(&json!({"Users":[{"IdentityStoreId":STORE,"UserId":USER,"UserName":"user"}],"NextToken":"opaque"}).to_string());}route(req)}).await;
    let s = p.collect().await.unwrap();
    assert!(!s.complete);
    assert_eq!(s.accounts.len(), 1);
    assert_eq!(s.grants.len(), 2);
    t.abort();
}
#[tokio::test]
async fn malformed_or_reflected_success_never_exports_credentials() {
    for payload in [
        json!({}),
        json!({"Users":null}),
        json!({"Users":[{"IdentityStoreId":STORE,"UserId":USER,"UserName":"SYNTHETIC_SECRET"}]}),
        json!({"Users":[{"IdentityStoreId":"d-9999999999","UserId":USER}]} ),
    ] {
        let (p, t) = mock(config(), move |req| {
            if req.contains("ListUsers") {
                response(&payload.to_string())
            } else {
                route(req)
            }
        })
        .await;
        match p.collect().await {
            Ok(s) => {
                assert!(!s.complete);
                assert!(s.accounts.is_empty());
                assert!(!format!("{s:?}").contains("SYNTHETIC_SECRET"));
            }
            Err(e) => assert!(!e.message.contains("SYNTHETIC_SECRET")),
        };
        t.abort();
    }
}
#[tokio::test]
async fn assignments_never_escape_allowlist_or_infer_missing_principals() {
    for mode in 0..2 {
        let(p,t)=mock(config(),move|req|{if req.contains("ListAccountAssignments"){return response(&json!({"AccountAssignments":[{"AccountId":if mode==0{"999999999999"}else{ACCOUNT},"PermissionSetArn":PERMISSION,"PrincipalId":"99999999-9999-9999-9999-999999999999","PrincipalType":"USER"}]}).to_string());}route(req)}).await;
        if mode == 0 {
            assert_eq!(p.collect().await.unwrap_err().code, "malformed");
        } else {
            let s = p.collect().await.unwrap();
            assert!(!s.complete);
            assert!(s.grants.is_empty());
        }
        t.abort();
    }
}
#[tokio::test]
async fn optional_organizations_only_decorates_allowlist() {
    let mut c = config();
    c.include_organizations = true;
    let(p,t)=mock(c,|req|{if req.contains("ListAccounts"){assert!(req.contains("\"MaxResults\":20"));return response(&json!({"Accounts":[{"Id":ACCOUNT,"Name":"Reviewed account"},{"Id":"333333333333","Name":"Unapproved"}]}).to_string());}assert!(!req.contains("333333333333"));route(req)}).await;
    let s = p.collect().await.unwrap();
    assert!(s.complete);
    assert!(s.resources.iter().any(|r| r.name == "Reviewed account"));
    assert!(!s.resources.iter().any(|r| r.name == "Unapproved"));
    t.abort();
}
#[test]
fn scope_tokens_and_temporary_credentials_are_bounded() {
    let mut pager = discovery::Pager::default();
    assert!(pager.advance(Some("opaque")).unwrap());
    assert!(pager.advance(Some("opaque")).is_err());
    assert!(discovery::Pager::default().advance(Some("")).is_err());
    for change in 0..4 {
        let mut c = config();
        match change {
            0 => c.accounts.clear(),
            1 => c.accounts.push(ACCOUNT.into()),
            2 => c.instance_arn = PERMISSION.into(),
            _ => c.caller_role = Some("path/role".into()),
        };
        assert!(!c.valid());
    }
    let c = config();
    let k = Credentials::new(
        "AKIATEST1234567890123",
        "SYNTHETIC_SECRET",
        None,
        None,
        "test",
    );
    assert!(IdentityCenterProvider::new("test".into(), c, k, None).is_err());
}
#[tokio::test]
async fn network_negotiation_host_decodes_sdk_records() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (p, t) = mock(config(), route).await;
    let (mut host, peer) = tokio::io::duplex(128 * 1024);
    let (reader, writer) = tokio::io::split(peer);
    let task = tokio::spawn(async move {
        permesh_native_runtime::serve_with_network::<protocol::IdentityCenter, _, _, _, _, _>(
            reader,
            writer,
            |_, _, _, _| async move { Ok(p) },
        )
        .await
    });
    let frames = format!(
        "{}\n{}\n",
        json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"center","operation":"discover","features":["network_v1"]}),
        json!({"protocol_version":1,"id":"discover","method":"discover","configuration":{"account_id":CALLER,"region":"eu-west-1","instance_arn":INSTANCE,"identity_store_id":STORE,"accounts":[ACCOUNT]},"credentials":{"access_key_id":"ASIATEST1234567890123","secret_access_key":"SYNTHETIC_SECRET","session_token":"SYNTHETIC_SESSION"},"network":{"https_proxy":"http://127.0.0.1:1234","no_proxy":["127.0.0.1"]}})
    );
    host.write_all(frames.as_bytes()).await.unwrap();
    let mut bytes = vec![];
    host.read_to_end(&mut bytes).await.unwrap();
    assert!(task.await.unwrap().is_ok());
    assert!(!String::from_utf8_lossy(&bytes).contains("SYNTHETIC_SECRET"));
    let mut decoder =
        permesh_provider_protocol::negotiated::DiscoveryDecoder::with_required_features(
            "aws-identity-center",
            "center",
            Some(&provider_metadata().capabilities),
            &[permesh_provider_protocol::negotiated::Feature::NetworkV1],
        )
        .unwrap();
    for frame in bytes.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    let s = decoder.finish().unwrap();
    assert_eq!(s.grants.len(), 2);
    t.abort();
}

#[tokio::test]
async fn repeated_continuation_and_oversized_body_remain_partial() {
    for oversized in [false, true] {
        let (p, t) = mock(config(), move |req| {
            if req.contains("ListUsers") {
                if oversized {
                    return response(&"x".repeat(2 * 1024 * 1024 + 1));
                }
                return response(&json!({"Users":[],"NextToken":"same"}).to_string());
            }
            route(req)
        })
        .await;
        let snapshot = p.collect().await.unwrap();
        assert!(!snapshot.complete);
        assert!(snapshot.accounts.is_empty());
        assert!(
            snapshot
                .grants
                .iter()
                .all(|g| matches!(g.subject, permesh_core::Subject::Group(_)))
        );
        t.abort();
    }
}
#[tokio::test]
async fn matching_caller_role_accepts_session_refresh() {
    let mut c = config();
    c.caller_role = Some("Review".into());
    let (p, t) = mock(c, route).await;
    assert!(p.collect().await.unwrap().complete);
    t.abort();
    assert!(crate::caller_matches(
        &format!("arn:aws:sts::{CALLER}:assumed-role/Review/new-session"),
        CALLER,
        Some("Review")
    ));
    assert!(!crate::caller_matches(
        &format!("arn:aws:sts::{CALLER}:assumed-role/Review/sub/session"),
        CALLER,
        Some("Review")
    ));
}
