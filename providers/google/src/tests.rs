// SPDX-License-Identifier: MIT
use super::*;
use permesh_core::{IdentityKind, IdentityStatus};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn user(id: &str) -> Value {
    json!({"id":id,"customerId":"C123","primaryEmail":format!("User{id}@example.com"),"suspended":false,"archived":false})
}
async fn mock(responses: Vec<(u16, String, String)>) -> (GoogleProvider, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/admin/directory/v1/users",
        listener.local_addr().unwrap()
    );
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    tokio::spawn(async move {
        for (status, headers, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buf = [0; 2048];
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|v| v == b"\r\n\r\n") {
                    break;
                }
            }
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8(request).unwrap());
            let response = format!(
                "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    let mut provider = GoogleProvider::new(
        "directory".into(),
        "C123".into(),
        Secret::new("private-token".into()),
    )
    .unwrap();
    provider.endpoint = Url::parse(&endpoint).unwrap();
    (provider, requests)
}
fn ok(value: Value) -> (u16, String, String) {
    (200, String::new(), value.to_string())
}
#[test]
fn rejects_ambiguous_customer_and_bad_credentials() {
    for customer in ["", "my_customer", "c123", "C", "Cfoo/bar"] {
        assert!(
            GoogleProvider::new("dir".into(), customer.into(), Secret::new("token".into()))
                .is_err()
        );
    }
    assert!(GoogleProvider::new("dir".into(), "C123".into(), Secret::new("".into())).is_err());
}
#[tokio::test]
async fn stable_directory_identity_and_minimal_read_only_request() {
    let mut row = user("1");
    row["aliases"] = json!(["alias@example.com"]);
    row["recoveryEmail"] = json!("recovery@example.com");
    let (provider, requests) = mock(vec![ok(json!({"users":[row]}))]).await;
    let snapshot = provider.discover().await.unwrap();
    assert!(snapshot.complete);
    assert_eq!(snapshot.identities[0].id, "google:C123:1");
    assert_eq!(snapshot.identities[0].status, IdentityStatus::Active);
    assert_eq!(snapshot.identities[0].kind, IdentityKind::Unknown);
    assert_eq!(
        snapshot.identities[0].verified_emails,
        ["User1@example.com"]
    );
    assert_eq!(snapshot.accounts[0].key.provider, "directory");
    assert!(
        snapshot.resources.is_empty() && snapshot.grants.is_empty() && snapshot.groups.is_empty()
    );
    snapshot.validate().unwrap();
    let request = &requests.lock().unwrap()[0];
    assert!(request.starts_with("GET /admin/directory/v1/users?"));
    assert!(
        request.contains("customer=C123")
            && request.contains("maxResults=500")
            && request.contains("projection=basic")
            && request.contains("viewType=admin_view")
    );
    assert!(request.contains("authorization: Bearer private-token"));
}
#[tokio::test]
async fn status_is_conservative_and_tenant_mismatch_excluded() {
    let mut rows: Vec<Value> = (1..=6).map(|id| user(&id.to_string())).collect();
    rows[1].as_object_mut().unwrap().remove("archived");
    rows[2]["suspended"] = json!(true);
    rows[3]["archived"] = json!(true);
    rows[4]["suspended"] = json!("false");
    rows[5]["customerId"] = json!("Cother");
    let (provider, _) = mock(vec![ok(json!({"users":rows}))]).await;
    let snapshot = provider.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.identities.len(), 5);
    assert_eq!(
        snapshot
            .identities
            .iter()
            .map(|v| v.status)
            .collect::<Vec<_>>(),
        [
            IdentityStatus::Active,
            IdentityStatus::Unknown,
            IdentityStatus::Inactive,
            IdentityStatus::Inactive,
            IdentityStatus::Unknown
        ]
    );
}
#[tokio::test]
async fn pagination_encodes_opaque_tokens_and_detects_cycles() {
    let token = "opaque+/=&?https://evil.example";
    let (provider, requests) = mock(vec![
        ok(json!({"users":[user("1")],"nextPageToken":token})),
        ok(json!({"users":[user("2")],"nextPageToken":token})),
    ])
    .await;
    let snapshot = provider.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.identities.len(), 2);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let target = requests[1].split_whitespace().nth(1).unwrap();
    let url = Url::parse(&format!("http://localhost{target}")).unwrap();
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "pageToken")
            .unwrap()
            .1,
        token
    );
}
#[tokio::test]
async fn duplicates_remove_potentially_stale_active_identity() {
    let mut inactive = user("1");
    inactive["suspended"] = json!(true);
    let (provider, _) = mock(vec![ok(json!({"users":[user("1"),inactive,user("1")]}))]).await;
    let snapshot = provider.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert!(snapshot.identities.is_empty() && snapshot.accounts.is_empty());
}
#[tokio::test]
async fn malformed_first_pages_fail_but_prior_success_is_preserved() {
    for body in ["null", "[]", "", "{", "{\"users\":null}"] {
        let (provider, _) = mock(vec![(200, String::new(), body.into())]).await;
        assert!(provider.discover().await.is_err(), "{body}");
        let (provider, _) = mock(vec![
            ok(json!({"users":[user("1")],"nextPageToken":"next"})),
            (200, String::new(), body.into()),
        ])
        .await;
        let snapshot = provider.discover().await.unwrap();
        assert!(!snapshot.complete);
        assert_eq!(snapshot.identities.len(), 1);
    }
    for value in [json!({}), json!({"users":[]})] {
        let (provider, _) = mock(vec![ok(value)]).await;
        assert!(provider.discover().await.unwrap().complete);
    }
}
#[tokio::test]
async fn auth_failures_and_redirects_are_sanitized_and_not_retried() {
    for (status, code) in [(401, "unauthorized"), (403, "forbidden"), (302, "http")] {
        let (provider, requests) = mock(vec![(
            status,
            "Location: https://evil.example\r\n".into(),
            "private-token remote-sensitive-message".into(),
        )])
        .await;
        let err = provider.discover().await.unwrap_err();
        assert_eq!(err.code, code);
        assert!(!format!("{err:?}").contains("private-token"));
        assert!(!err.message.contains("remote-sensitive"));
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn retries_only_quota_forbidden_and_bounds_retry_after() {
    for status in [429, 503, 403] {
        let (provider, requests) = mock(vec![
            (
                status,
                "Retry-After: 0\r\n".into(),
                json!({"error":{"errors":[{"reason":"userRateLimitExceeded"}]}}).to_string(),
            ),
            ok(json!({})),
        ])
        .await;
        assert!(provider.discover().await.unwrap().complete);
        assert_eq!(requests.lock().unwrap().len(), 2);
    }
    let (provider, requests) = mock(vec![(429, "Retry-After: 3600\r\n".into(), "{}".into())]).await;
    assert_eq!(provider.discover().await.unwrap_err().code, "rate_limit");
    assert_eq!(requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn check_probes_once_and_validates_tenant() {
    let (provider, requests) = mock(vec![ok(
        json!({"users":[user("1")],"nextPageToken":"unused"}),
    )])
    .await;
    assert!(provider.check().await.is_ok());
    assert!(requests.lock().unwrap()[0].contains("maxResults=1"));
    let mut row = user("1");
    row["customerId"] = json!("Cother");
    let (provider, _) = mock(vec![ok(json!({"users":[row]}))]).await;
    assert!(provider.check().await.is_err());
}

#[tokio::test]
async fn response_and_page_limits_preserve_bounded_records() {
    let (provider, _) = mock(vec![(200, String::new(), " ".repeat(2 * 1024 * 1024 + 1))]).await;
    assert_eq!(provider.discover().await.unwrap_err().code, "limit");
    let rows: Vec<_> = (0..501).map(|id| user(&id.to_string())).collect();
    let (provider, _) = mock(vec![ok(json!({"users":rows}))]).await;
    let snapshot = provider.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.identities.len(), 500);
    let responses = (0..200)
        .map(|page| {
            ok(json!({"users":[user(&page.to_string())],"nextPageToken":format!("page{}",page+1)}))
        })
        .collect();
    let (provider, requests) = mock(responses).await;
    let snapshot = provider.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert_eq!(snapshot.identities.len(), 200);
    assert_eq!(requests.lock().unwrap().len(), 200);
}
#[tokio::test]
async fn invalid_tokens_stop_after_successful_page() {
    for token in [
        json!(null),
        json!(42),
        json!(""),
        json!("bad\n"),
        json!("x".repeat(4097)),
    ] {
        let (provider, requests) =
            mock(vec![ok(json!({"users":[user("1")],"nextPageToken":token}))]).await;
        let snapshot = provider.discover().await.unwrap();
        assert!(!snapshot.complete);
        assert_eq!(snapshot.identities.len(), 1);
        assert_eq!(requests.lock().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn invalid_records_cannot_create_email_or_identity_claims() {
    let mut rows = Vec::new();
    for (key, value) in [
        ("id", json!("")),
        ("id", json!(42)),
        ("id", json!("a:b")),
        ("primaryEmail", json!("wrong")),
        ("primaryEmail", json!("u ser@example.com")),
        ("primaryEmail", json!("user@example.com\n")),
        ("customerId", json!(null)),
    ] {
        let mut row = user(&rows.len().to_string());
        row[key] = value;
        rows.push(row);
    }
    let (provider, _) = mock(vec![ok(json!({"users":rows}))]).await;
    let snapshot = provider.discover().await.unwrap();
    assert!(!snapshot.complete);
    assert!(snapshot.identities.is_empty());
}
#[tokio::test]
async fn retry_budget_and_cancellation_are_bounded() {
    let response = (503, "Retry-After: 0\r\n".into(), "{}".into());
    let (provider, requests) = mock(vec![response.clone(), response.clone(), response]).await;
    assert_eq!(provider.discover().await.unwrap_err().code, "http");
    assert_eq!(requests.lock().unwrap().len(), 3);
    let (provider, requests) = mock(vec![
        (429, "Retry-After: 5\r\n".into(), "{}".into()),
        ok(json!({})),
    ])
    .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), provider.discover())
            .await
            .is_err()
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(requests.lock().unwrap().len(), 1);
    let date = (time::OffsetDateTime::now_utc() + time::Duration::hours(1))
        .format(&time::format_description::well_known::Rfc2822)
        .unwrap();
    let (provider, requests) =
        mock(vec![(429, format!("Retry-After: {date}\r\n"), "{}".into())]).await;
    assert_eq!(provider.discover().await.unwrap_err().code, "rate_limit");
    assert_eq!(requests.lock().unwrap().len(), 1);
    let (provider, requests) = mock(vec![(
        403,
        "Retry-After: 0\r\n".into(),
        json!({"error":{"errors":[{"reason":"forbidden"}]}}).to_string(),
    )])
    .await;
    assert_eq!(provider.discover().await.unwrap_err().code, "forbidden");
    assert_eq!(requests.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn truncated_body_fails_and_slow_body_obeys_deadline() {
    for slow in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 8192];
            let _ = socket.read(&mut buf).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
                .await
                .unwrap();
            if slow {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
        let mut provider = GoogleProvider::new(
            "directory".into(),
            "C123".into(),
            Secret::new("private-token".into()),
        )
        .unwrap();
        provider.endpoint =
            Url::parse(&format!("http://{address}/admin/directory/v1/users")).unwrap();
        provider.request_timeout = Duration::from_millis(50);
        assert_eq!(
            provider.discover().await.unwrap_err().code,
            if slow { "timeout" } else { "transport" }
        );
        task.abort();
    }
}

#[tokio::test]
async fn canonical_alias_survives_primary_email_rename_and_instance_change() {
    use permesh_core::{Aliases, query_user};
    let aliases: Aliases = BTreeMap::from([(
        "google:C123:123".into(),
        BTreeMap::from([("github".into(), vec!["456".into()])]),
    )]);
    for (instance, email) in [
        ("directory", "first@example.com"),
        ("renamed-directory", "renamed@example.com"),
    ] {
        let mut row = user("123");
        row["primaryEmail"] = json!(email);
        let (mut provider, _) = mock(vec![ok(json!({"users":[row]}))]).await;
        provider.id = instance.into();
        let directory = provider.discover().await.unwrap();
        let mut github = Snapshot::new("github");
        github.accounts.push(Account {
            key: EntityKey::new("github", "456"),
            login: "octocat".into(),
            kind: IdentityKind::Human,
            verified_emails: vec![],
        });
        let snapshots = [directory, github];
        let result = query_user(&snapshots, &aliases, "octocat").unwrap();
        assert_eq!(result.identity.unwrap().id, "google:C123:123");
        assert_eq!(result.accounts.len(), 2);
        assert_eq!(
            query_user(&snapshots, &aliases, email)
                .unwrap()
                .accounts
                .len(),
            2
        );
        assert!(
            query_user(&snapshots, &Aliases::new(), "octocat")
                .unwrap()
                .identity
                .is_none()
        );
        assert!(query_user(&snapshots, &aliases, "FIRST@example.com").is_err());
    }
}
