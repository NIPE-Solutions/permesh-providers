// SPDX-License-Identifier: MIT
#![allow(clippy::unwrap_used)]
use super::*;
use permesh_core::{Affiliation, IdentityKind, IdentityStatus, Subject};
use permesh_provider_sdk::Provider;
use permesh_secrets::Secret;
use reqwest::Url;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
pub(super) const TENANT: &str = "11111111-1111-1111-1111-111111111111";
pub(super) const USER: &str = "22222222-2222-2222-2222-222222222222";
pub(super) const GROUP: &str = "33333333-3333-3333-3333-333333333333";
pub(super) const SERVICE: &str = "44444444-4444-4444-4444-444444444444";
pub(super) async fn mock<F>(
    include_service_principals: bool,
    handler: F,
) -> (EntraProvider, tokio::task::JoinHandle<()>)
where
    F: Fn(&str) -> (u16, String, String) + Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut bytes = vec![0; 32768];
            let n = stream.read(&mut bytes).await.unwrap();
            let req = String::from_utf8_lossy(&bytes[..n]);
            assert!(req.starts_with("GET "));
            assert!(
                req.to_ascii_lowercase().contains(
                    "authorization: bearer SYNTHETIC_TOKEN"
                        .to_ascii_lowercase()
                        .as_str()
                )
            );
            let (status, headers, body) = handler(&req);
            let response = format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    });
    let mut provider = EntraProvider::new(
        "entra".into(),
        TENANT.into(),
        include_service_principals,
        Secret::new("SYNTHETIC_TOKEN".into()),
    )
    .unwrap();
    provider.origin = Url::parse(&format!("http://{address}/v1.0/")).unwrap();
    (provider, task)
}
pub(super) fn value(rows: Value) -> (u16, String, String) {
    (200, String::new(), json!({"value":rows}).to_string())
}
pub(super) fn routes(req: &str) -> (u16, String, String) {
    let path = req
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    match path {
        "/v1.0/organization" => value(json!([{"id":TENANT}])),
        "/v1.0/users" => value(
            json!([{"id":USER,"displayName":"Departing contractor","userPrincipalName":"unverified@example.com","userType":"Guest","accountEnabled":false}]),
        ),
        "/v1.0/groups" => value(json!([{"id":GROUP,"displayName":"Reviewers"}])),
        "/v1.0/servicePrincipals" => value(
            json!([{"id":SERVICE,"displayName":"Build app","servicePrincipalType":"Application","accountEnabled":true}]),
        ),
        _ if path == format!("/v1.0/groups/{GROUP}/members") => {
            value(json!([{"id":USER,"@odata.type":"#microsoft.graph.user"}]))
        }
        _ => panic!("unexpected mock route {path}"),
    }
}
#[tokio::test]
async fn tenant_proof_precedes_directory_and_records_preserve_uncertainty() {
    let (p, task) = mock(true, routes).await;
    let snapshot = p.discover().await.unwrap();
    snapshot.validate().unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.identities.len(), 2);
    assert_eq!(snapshot.accounts.len(), 2);
    assert!(snapshot.grants.is_empty() && snapshot.resources.is_empty());
    let user = snapshot
        .accounts
        .iter()
        .find(|a| a.key.id.ends_with(USER))
        .unwrap();
    assert_eq!(user.kind, IdentityKind::Unknown);
    assert_eq!(user.status, IdentityStatus::Inactive);
    assert_eq!(user.affiliation, Affiliation::External);
    assert!(user.verified_emails.is_empty());
    assert!(user.key.id.contains(TENANT));
    assert!(matches!(&snapshot.memberships[0].member,Subject::Account(k) if k==&user.key));
    assert_eq!(
        snapshot.memberships[0].provenance.method,
        "entra.direct_group_membership"
    );
    assert!(
        snapshot
            .limitations
            .iter()
            .any(|s| s.contains("service-principal group memberships"))
    );
    task.abort();
}
#[tokio::test]
async fn wrong_tenant_prevents_all_other_requests_without_reflection() {
    let (p, task) = mock(false, |req| {
        assert!(req.contains("/v1.0/organization"));
        value(json!([{"id":SERVICE,"displayName":"REMOTE_SECRET"}]))
    })
    .await;
    let error = p.discover().await.unwrap_err();
    assert_eq!(error.code, "scope");
    assert!(!error.message.contains("REMOTE_SECRET"));
    task.abort();
}
