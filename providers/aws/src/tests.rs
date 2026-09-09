// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use super::*;
use permesh_core::{Affiliation, Certainty, EvidenceKind, IdentityKind, IdentityStatus, Privilege};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const ACCOUNT: &str = "123456789012";
const USER: &str = "AIDAEXAMPLEUSER1234567";
const GROUP: &str = "AGPAEXAMPLEGROUP12345";
const POLICY: &str = "ANPAEXAMPLEPOLICY1234";
fn identity(account: &str) -> String {
    format!(
        "<GetCallerIdentityResponse xmlns=\"https://sts.amazonaws.com/doc/2011-06-15/\"><GetCallerIdentityResult><Arn>arn:aws:iam::{account}:user/reader</Arn><UserId>{USER}</UserId><Account>{account}</Account></GetCallerIdentityResult><ResponseMetadata><RequestId>request</RequestId></ResponseMetadata></GetCallerIdentityResponse>"
    )
}
fn page(users: &str, groups: &str, policies: &str, tail: &str) -> String {
    format!(
        "<GetAccountAuthorizationDetailsResponse xmlns=\"https://iam.amazonaws.com/doc/2010-05-08/\"><GetAccountAuthorizationDetailsResult><UserDetailList>{users}</UserDetailList><GroupDetailList>{groups}</GroupDetailList><RoleDetailList/><Policies>{policies}</Policies>{tail}</GetAccountAuthorizationDetailsResult><ResponseMetadata><RequestId>request</RequestId></ResponseMetadata></GetAccountAuthorizationDetailsResponse>"
    )
}
fn user(id: &str, account: &str, name: &str) -> String {
    format!(
        "<member><UserId>{id}</UserId><UserName>{name}</UserName><Arn>arn:aws:iam::{account}:user/{name}</Arn><GroupList><member>operators</member></GroupList><UserPolicyList><member><PolicyName>inline-deny</PolicyName><PolicyDocument>%7B%22Statement%22%3A%7B%22Effect%22%3A%22Deny%22%7D%7D</PolicyDocument></member></UserPolicyList><AttachedManagedPolicies><member><PolicyName>AdministratorAccess</PolicyName><PolicyArn>arn:aws:iam::aws:policy/AdministratorAccess</PolicyArn></member></AttachedManagedPolicies></member>"
    )
}
fn group() -> String {
    format!(
        "<member><GroupId>{GROUP}</GroupId><GroupName>operators</GroupName><Arn>arn:aws:iam::{ACCOUNT}:group/operators</Arn><AttachedManagedPolicies><member><PolicyName>AdministratorAccess</PolicyName><PolicyArn>arn:aws:iam::aws:policy/AdministratorAccess</PolicyArn></member></AttachedManagedPolicies></member>"
    )
}
fn policy() -> String {
    format!(
        "<member><PolicyId>{POLICY}</PolicyId><PolicyName>AdministratorAccess</PolicyName><Arn>arn:aws:iam::aws:policy/AdministratorAccess</Arn><DefaultVersionId>v1</DefaultVersionId></member>"
    )
}
async fn mock<F>(handler: F) -> (AwsProvider, tokio::task::JoinHandle<()>)
where
    F: Fn(&str, &str) -> (u16, String, String) + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let n = socket.read(&mut buffer).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8_lossy(&request);
            let (head, body) = request.split_once("\r\n\r\n").unwrap();
            assert!(head.starts_with("POST "));
            assert!(
                head.to_ascii_lowercase()
                    .contains("authorization: aws4-hmac-sha256")
            );
            let (status, headers, response) = handler(head, body);
            let reply = format!(
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: text/xml\r\nConnection: close\r\n{headers}\r\n{response}",
                response.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    let credentials = Credentials::new(
        "AKIATESTEXAMPLE123456",
        "secret-key-sentinel",
        Some("session-token-sentinel".into()),
        None,
        "tests",
    );
    let provider = AwsProvider::build(
        "aws-main".into(),
        ACCOUNT.into(),
        "eu-west-1".into(),
        credentials,
        format!("{origin}/iam"),
        format!("{origin}/sts"),
    )
    .unwrap();
    (provider, task)
}
#[tokio::test]
async fn signed_readonly_calls_preserve_attachment_evidence_without_effective_privilege() {
    let (p, t) = mock(|head, body| {
        let response = if body.contains("Action=GetCallerIdentity") {
            assert!(head.contains("/eu-west-1/sts/aws4_request"));
            identity(ACCOUNT)
        } else {
            assert!(body.contains("Action=GetAccountAuthorizationDetails"));
            assert!(body.contains("MaxItems=100"));
            assert!(head.contains("/us-east-1/iam/aws4_request"));
            page(
                &user(USER, ACCOUNT, "alice"),
                &group(),
                &policy(),
                "<IsTruncated>false</IsTruncated>",
            )
        };
        (200, String::new(), response)
    })
    .await;
    let snapshot = p.discover().await.unwrap();
    snapshot.validate().unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 1);
    assert!(snapshot.identities.is_empty());
    assert!(snapshot.accounts[0].verified_emails.is_empty());
    assert_eq!(snapshot.accounts[0].kind, IdentityKind::Unknown);
    assert_eq!(snapshot.accounts[0].status, IdentityStatus::Unknown);
    assert_eq!(snapshot.accounts[0].affiliation, Affiliation::Unknown);
    assert_eq!(snapshot.resources.len(), 2);
    for resource in &snapshot.resources {
        assert!(resource.parent.is_none());
        assert_eq!(
            resource.kind.as_deref(),
            Some(if resource.key.id.starts_with("inline:") {
                "aws.inline_policy"
            } else {
                "aws.managed_policy"
            })
        );
    }
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(snapshot.memberships.len(), 1);
    assert_eq!(snapshot.grants.len(), 3);
    assert!(
        snapshot
            .grants
            .iter()
            .all(|g| g.privilege == Privilege::Unknown
                && g.certainty == Certainty::Observed
                && g.evidence_kind == EvidenceKind::PolicyAttachment)
    );
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("Statement")
    );
    t.abort();
}
#[tokio::test]
async fn account_mismatch_stops_before_iam() {
    let (p, t) = mock(|_, body| {
        assert!(body.contains("GetCallerIdentity"));
        (200, String::new(), identity("999999999999"))
    })
    .await;
    assert!(p.discover().await.is_err());
    t.abort();
}
#[tokio::test]
async fn short_truncated_pages_resolve_groups_and_policies_collected_later() {
    let (p, t) = mock(|_, body| {
        let response = if body.contains("GetCallerIdentity") {
            identity(ACCOUNT)
        } else if body.contains("Marker=next") {
            page("", &group(), &policy(), "<IsTruncated>false</IsTruncated>")
        } else {
            page(
                &user(USER, ACCOUNT, "alice"),
                "",
                "",
                "<IsTruncated>true</IsTruncated><Marker>next</Marker>",
            )
        };
        (200, String::new(), response)
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(s.complete);
    assert_eq!(s.memberships.len(), 1);
    assert_eq!(s.grants.len(), 3);
    t.abort();
}
#[tokio::test]
async fn missing_policies_foreign_accounts_and_conflicting_ids_cannot_form_paths() {
    let (p, t) = mock(|_, body| {
        let response = if body.contains("GetCallerIdentity") {
            identity(ACCOUNT)
        } else {
            page(
                &format!(
                    "{}{}{}",
                    user(USER, ACCOUNT, "alice"),
                    user(USER, ACCOUNT, "changed"),
                    user("AIDAOTHERUSER12345678", "999999999999", "foreign")
                ),
                &group(),
                "",
                "<IsTruncated>false</IsTruncated>",
            )
        };
        (200, String::new(), response)
    })
    .await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(!s.complete);
    assert!(s.accounts.is_empty());
    assert!(s.grants.is_empty());
    t.abort();
}
#[tokio::test]
async fn iam_denial_after_successful_page_retains_prior_observations_safely() {
    let(p,t)=mock(|_,body|{if body.contains("GetCallerIdentity"){return(200,String::new(),identity(ACCOUNT))}
if body.contains("Marker=next"){return(403,String::new(),"<ErrorResponse><Error><Code>AccessDenied</Code><Message>secret-key-sentinel</Message></Error></ErrorResponse>".into())}(200,String::new(),page(&user(USER,ACCOUNT,"alice"),&group(),&policy(),"<IsTruncated>true</IsTruncated><Marker>next</Marker>"))}).await;
    let s = p.discover().await.unwrap();
    s.validate().unwrap();
    assert!(!s.complete);
    assert_eq!(s.accounts.len(), 1);
    assert!(
        !serde_json::to_string(&s)
            .unwrap()
            .contains("secret-key-sentinel")
    );
    t.abort();
}
#[tokio::test]
async fn malformed_missing_pagination_signal_is_not_complete() {
    let (p, t) = mock(|_, body| {
        (
            200,
            String::new(),
            if body.contains("GetCallerIdentity") {
                identity(ACCOUNT)
            } else {
                page("", "", "", "")
            },
        )
    })
    .await;
    assert!(p.discover().await.is_err());
    t.abort();
}
#[tokio::test]
async fn repeated_or_absent_markers_stop_with_partial_evidence() {
    for tail in [
        "<IsTruncated>true</IsTruncated>",
        "<IsTruncated>true</IsTruncated><Marker>again</Marker>",
    ] {
        let (p, t) = mock(move |_, body| {
            (
                200,
                String::new(),
                if body.contains("GetCallerIdentity") {
                    identity(ACCOUNT)
                } else {
                    page(&user(USER, ACCOUNT, "alice"), &group(), &policy(), tail)
                },
            )
        })
        .await;
        let s = p.discover().await.unwrap();
        assert!(!s.complete);
        assert_eq!(s.accounts.len(), 1);
        t.abort();
    }
}
#[tokio::test]
async fn reflected_credentials_are_omitted_and_mark_partial() {
    let (p, t) = mock(|_, body| {
        (
            200,
            String::new(),
            if body.contains("GetCallerIdentity") {
                identity(ACCOUNT)
            } else {
                page(
                    &user(USER, ACCOUNT, "session-token-sentinel"),
                    "",
                    "",
                    "<IsTruncated>false</IsTruncated>",
                )
            },
        )
    })
    .await;
    let s = p.discover().await.unwrap();
    assert!(!s.complete);
    assert!(s.accounts.is_empty());
    assert!(
        !serde_json::to_string(&s)
            .unwrap()
            .contains("session-token-sentinel")
    );
    t.abort();
}
#[tokio::test]
async fn redirect_and_oversized_responses_are_not_followed_or_reflected() {
    for status in [302, 200] {
        let (p, t) = mock(move |_, _| {
            (
                status,
                "Location: http://127.0.0.1:1/secret-key-sentinel\r\n".into(),
                if status == 200 {
                    "x".repeat(2 * 1024 * 1024 + 1)
                } else {
                    "secret-key-sentinel".into()
                },
            )
        })
        .await;
        let e = p.discover().await.unwrap_err();
        assert!(!format!("{e:?}").contains("secret-key-sentinel"));
        t.abort();
    }
}
#[tokio::test]
async fn iam_throttling_retries_are_bounded() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let(p,t)=mock(move|_,body|if body.contains("GetCallerIdentity"){(200,String::new(),identity(ACCOUNT))}else{counter.fetch_add(1,Ordering::SeqCst);(400,String::new(),"<ErrorResponse><Error><Code>Throttling</Code><Message>secret-key-sentinel</Message></Error></ErrorResponse>".into())}).await;
    let e = p.discover().await.unwrap_err();
    assert_eq!(e.code, "rate_limit");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    t.abort();
}
#[tokio::test]
async fn roles_are_native_unknown_principals_without_assumption_edges() {
    let(p,t)=mock(|_,body|(200,String::new(),if body.contains("GetCallerIdentity"){identity(ACCOUNT)}else{page("","",&policy(),"<IsTruncated>false</IsTruncated>").replace("<RoleDetailList/>","<RoleDetailList><member><RoleId>AROAEXAMPLEROLE1234567</RoleId><RoleName>operator</RoleName><Arn>arn:aws:iam::123456789012:role/path/operator</Arn><AssumeRolePolicyDocument>secret-key-sentinel</AssumeRolePolicyDocument><AttachedManagedPolicies><member><PolicyName>AdministratorAccess</PolicyName><PolicyArn>arn:aws:iam::aws:policy/AdministratorAccess</PolicyArn></member></AttachedManagedPolicies></member></RoleDetailList>")})).await;
    let s = p.discover().await.unwrap();
    assert!(s.complete);
    assert_eq!(s.accounts.len(), 1);
    assert_eq!(s.accounts[0].kind, IdentityKind::Unknown);
    assert_eq!(s.accounts[0].status, IdentityStatus::Unknown);
    assert_eq!(s.accounts[0].affiliation, Affiliation::Unknown);
    assert!(
        s.grants
            .iter()
            .all(|g| g.evidence_kind == EvidenceKind::PolicyAttachment)
    );
    assert!(s.memberships.is_empty());
    assert_eq!(s.grants.len(), 1);
    assert!(!serde_json::to_string(&s).unwrap().contains("sentinel"));
    t.abort();
}
#[tokio::test]
async fn dropping_collection_closes_pending_body_and_stops_requests() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 8192];
        let _ = socket.read(&mut bytes).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\n")
            .await
            .unwrap();
        let n = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(n, 0);
    });
    let p = AwsProvider::build(
        "aws-main".into(),
        ACCOUNT.into(),
        "eu-west-1".into(),
        Credentials::new("AKIATEST1234567890123", "secret", None, None, "test"),
        endpoint.clone(),
        endpoint,
    )
    .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), p.discover())
            .await
            .is_err()
    );
    task.await.unwrap();
}
#[test]
fn public_configuration_cannot_select_ambient_or_noncommercial_endpoints() {
    for region in [
        "cn-north-1",
        "us-gov-west-1",
        "https://evil.invalid",
        "eu-west-1/path",
        "EU-west-1",
    ] {
        assert!(!valid_region(region));
    }
}
#[tokio::test]
async fn policy_arn_name_disagreement_cannot_create_attachment_evidence() {
    let (p, t) = mock(|_, body| {
        (
            200,
            String::new(),
            if body.contains("GetCallerIdentity") {
                identity(ACCOUNT)
            } else {
                page(
                    "",
                    &group(),
                    &policy().replace(":policy/AdministratorAccess", ":policy/OtherPolicy"),
                    "<IsTruncated>false</IsTruncated>",
                )
            },
        )
    })
    .await;
    let s = p.discover().await.unwrap();
    assert!(!s.complete);
    assert!(s.resources.is_empty());
    assert!(s.grants.is_empty());
    t.abort();
}

#[tokio::test]
async fn shared_runtime_cross_decodes_policy_attachments_and_resource_kinds() {
    use permesh_provider_protocol::negotiated::DiscoveryDecoder;
    let (provider, server) = mock(|_, body| {
        (
            200,
            String::new(),
            if body.contains("GetCallerIdentity") {
                identity(ACCOUNT)
            } else {
                page(
                    &user(USER, ACCOUNT, "alice"),
                    &group(),
                    &policy(),
                    "<IsTruncated>false</IsTruncated>",
                )
            },
        )
    })
    .await;
    let (mut host, peer) = tokio::io::duplex(128 * 1024);
    let (reader, writer) = tokio::io::split(peer);
    let task = tokio::spawn(async move {
        permesh_native_runtime::serve::<crate::protocol::Aws, _, _, _, _, _>(
            reader,
            writer,
            move |_, _, _| async move { Ok(provider) },
        )
        .await
    });
    let request = format!(
        "{}\n{}\n",
        serde_json::json!({"protocol_version":1,"id":"handshake","method":"handshake","instance":"aws-main","operation":"discover"}),
        serde_json::json!({"protocol_version":1,"id":"discover","method":"discover","configuration":{"account_id":ACCOUNT,"region":"eu-west-1"},"credentials":{"access_key_id":"AKIATEST1234567890123","secret_access_key":"SENTINEL"}})
    );
    host.write_all(request.as_bytes()).await.unwrap();
    let mut output = Vec::new();
    host.read_to_end(&mut output).await.unwrap();
    assert!(task.await.unwrap().is_ok());
    let mut decoder =
        DiscoveryDecoder::new("aws", "aws-main", Some(&provider_metadata().capabilities)).unwrap();
    for frame in output.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame).unwrap();
    }
    let snapshot = decoder.finish().unwrap();
    snapshot.validate().unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(snapshot.accounts[0].status, IdentityStatus::Unknown);
    assert_eq!(snapshot.accounts[0].affiliation, Affiliation::Unknown);
    assert_eq!(snapshot.grants.len(), 3);
    assert!(
        snapshot
            .grants
            .iter()
            .all(|g| g.evidence_kind == EvidenceKind::PolicyAttachment
                && g.certainty == Certainty::Observed
                && g.privilege == Privilege::Unknown)
    );
    assert_eq!(snapshot.resources.len(), 2);
    assert!(snapshot.resources.iter().all(|r| r.parent.is_none()));
    assert!(
        snapshot
            .resources
            .iter()
            .any(|r| r.kind.as_deref() == Some("aws.managed_policy"))
    );
    assert!(
        snapshot
            .resources
            .iter()
            .any(|r| r.kind.as_deref() == Some("aws.inline_policy"))
    );
    server.abort();
}

#[tokio::test]
async fn selected_caller_role_rejects_other_role_before_iam_inventory() {
    let (mut p, t) = mock(|_, body| {
        assert!(body.contains("GetCallerIdentity"));
        (
            200,
            String::new(),
            identity(ACCOUNT).replace(
                &format!("arn:aws:iam::{ACCOUNT}:user/reader"),
                &format!("arn:aws:sts::{ACCOUNT}:assumed-role/Other/session"),
            ),
        )
    })
    .await;
    p.caller_role = Some("Approved".into());
    assert_eq!(p.discover().await.unwrap_err().code, "account_mismatch");
    t.abort();
}
