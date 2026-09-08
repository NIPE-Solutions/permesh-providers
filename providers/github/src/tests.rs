// SPDX-License-Identifier: MIT OR Apache-2.0
#[cfg(test)]
mod tests {
    use crate::client::{Budget, next_page, retry_delay};
    use crate::*;
    use permesh_core::Privilege;
    use permesh_provider_sdk::Provider;
    use reqwest::header::HeaderMap;
    use time::{OffsetDateTime, format_description::well_known::Rfc2822};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn mock<F>(handler: F) -> (String, tokio::task::JoinHandle<()>)
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
                let target = request.split_whitespace().nth(1).unwrap();
                let (status, headers, body) = handler(target);
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{headers}\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        (format!("http://{address}"), task)
    }

    fn provider(origin: &str) -> GithubProvider {
        let mut provider = GithubProvider::new(
            "github".into(),
            vec!["acme".into()],
            permesh_secrets::Secret::new("private-test-token".into()),
        )
        .unwrap();
        provider.origin = reqwest::Url::parse(origin).unwrap();
        provider
    }

    #[tokio::test]
    async fn empty_discovery_is_valid_and_has_visibility_limits() {
        let (origin, task) = mock(|target| {
            (
                200,
                String::new(),
                if target == "/orgs/acme" {
                    r#"{"id":9,"login":"acme"}"#.into()
                } else {
                    "[]".into()
                },
            )
        })
        .await;
        let snapshot = provider(&origin).discover().await.unwrap();
        snapshot.validate().unwrap();
        assert!(snapshot.complete);
        assert!(!snapshot.limitations.is_empty());
        task.abort();
    }

    #[tokio::test]
    async fn unauthorized_and_forbidden_are_sanitized_errors() {
        for status in [401, 403] {
            let (origin, task) =
                mock(move |_| (status, String::new(), "private-test-token".into())).await;
            let error = provider(&origin).discover().await.unwrap_err();
            assert!(!error.to_string().contains("private-test-token"));
            assert!(provider(&origin).check().await.is_err());
            task.abort();
        }
    }

    #[tokio::test]
    async fn preserves_partial_pages_and_custom_effective_roles_without_email() {
        let (origin, task) = mock(|target| {
            let body = match target {
                "/orgs/acme" => r#"{"id":9,"login":"acme"}"#,
                "/orgs/acme/repos?per_page=100&page=1" => r#"[{"id":7,"name":"app","full_name":"acme/app"}]"#,
                "/repos/acme/app/collaborators?affiliation=all&per_page=100&page=1" => r#"[{"id":1,"login":"alice","type":"User","email":"alice@example.com","role_name":"custom-superuser"}]"#,
                _ => "[]",
            };
            if target.contains("/teams") { (403,String::new(),"secret".into()) }
            else { (200,String::new(),body.into()) }
        }).await;
        let snapshot = provider(&origin).discover().await.unwrap();
        snapshot.validate().unwrap();
        assert!(!snapshot.complete);
        assert_eq!(snapshot.accounts.len(), 1);
        assert!(snapshot.accounts[0].verified_emails.is_empty());
        let grant = snapshot
            .grants
            .iter()
            .find(|g| g.role == "custom-superuser")
            .unwrap();
        assert_eq!(grant.privilege, Privilege::Unknown);
        assert!(grant.provenance.method.contains("effective"));
        task.abort();
    }

    #[tokio::test]
    async fn pagination_constructs_safe_urls_and_keeps_first_page_on_failure() {
        let (origin, task) = mock(|target| {
            if target.ends_with("page=1") {
                (
                    200,
                    "Link: <https://evil.invalid/steal?page=2>; rel=\"next\"\r\n".into(),
                    "[{\"id\":1}]".into(),
                )
            } else {
                (403, String::new(), "secret".into())
            }
        })
        .await;
        let p = provider(&origin);
        let mut budget = Budget::default();
        let result = p.list(&["items"], &[], &mut budget).await;
        assert_eq!(result.items.len(), 1);
        assert!(result.error.is_some());
        task.abort();
    }

    #[tokio::test]
    async fn retries_rate_limit_then_succeeds_and_rejects_malformed_json() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = AtomicUsize::new(0);
        let (origin, task) = mock(move |_| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                (429, "Retry-After: 0\r\n".into(), "secret".into())
            } else {
                (200, String::new(), "[]".into())
            }
        })
        .await;
        let p = provider(&origin);
        let result = p.list(&["items"], &[], &mut Budget::default()).await;
        assert!(result.error.is_none());
        task.abort();
        let (origin, task) = mock(|_| (200, String::new(), "{invalid".into())).await;
        let result = provider(&origin)
            .list(&["items"], &[], &mut Budget::default())
            .await;
        assert!(result.error.is_some());
        task.abort();
    }

    #[tokio::test]
    async fn collects_team_roles_from_permission_endpoint_and_owner_role() {
        let (origin, task) = mock(|target| {
            let body = match target {
                "/orgs/acme" => r#"{"id":9,"login":"acme"}"#,
                "/orgs/acme/members?role=admin&per_page=100&page=1" => {
                    r#"[{"id":1,"login":"alice","type":"User"}]"#
                }
                "/orgs/acme/teams?per_page=100&page=1" => {
                    r#"[{"id":3,"slug":"eng","name":"Engineering"}]"#
                }
                "/orgs/acme/teams/eng/members?role=all&per_page=100&page=1" => {
                    r#"[{"id":1,"login":"alice","type":"User"}]"#
                }
                "/orgs/acme/teams/eng/repos?per_page=100&page=1" => {
                    r#"[{"id":7,"full_name":"acme/app","permissions":{"admin":true}}]"#
                }
                "/orgs/acme/teams/eng/repos/acme/app" => {
                    r#"{"id":7,"full_name":"acme/app","role_name":"security-reviewer"}"#
                }
                _ => "[]",
            };
            (200, String::new(), body.into())
        })
        .await;
        let snapshot = provider(&origin).discover().await.unwrap();
        snapshot.validate().unwrap();
        assert!(snapshot.complete);
        assert_eq!(snapshot.memberships.len(), 2);
        assert!(
            snapshot
                .grants
                .iter()
                .any(|g| g.role == "admin" && g.privilege == Privilege::Owner)
        );
        assert!(
            snapshot
                .grants
                .iter()
                .any(|g| g.role == "security-reviewer" && g.privilege == Privilege::Unknown)
        );
        task.abort();
    }

    #[tokio::test]
    async fn timeout_and_cancellation_stop_requests() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut p = provider(&origin);
        p.request_timeout = Duration::from_millis(20);
        let error = p.check().await.unwrap_err();
        assert_eq!(error.code, "timeout");
        let result = tokio::time::timeout(Duration::from_millis(5), p.check()).await;
        assert!(result.is_err());
    }

    #[test]
    fn pagination_validates_origin_path_query_and_full_last_page() {
        let current =
            Url::parse("https://api.github.com/orgs/acme/members?role=admin&per_page=100&page=1")
                .unwrap();
        let mut headers = HeaderMap::new();
        assert!(next_page(&current, &headers, 100).unwrap());
        assert!(!next_page(&current, &headers, 0).unwrap());
        headers.insert("link", "<https://api.github.com/orgs/acme/members?role=admin&per_page=100&page=2>; rel=\"next\"".parse().unwrap());
        assert!(next_page(&current, &headers, 1).unwrap());
        for value in [
            "<https://evil.invalid/path>; rel=\"next\"",
            "<https://api.github.com/user?page=2>; rel=\"next\"",
            "malformed",
            "<https://api.github.com/orgs/acme/members?role=admin&per_page=100&page=1>; rel=\"next\"",
        ] {
            headers.insert("link", value.parse().unwrap());
            assert!(next_page(&current, &headers, 100).is_err());
        }
        headers.insert(
            "link",
            "<https://api.github.com/orgs/acme/members?page=1>; rel=\"prev\""
                .parse()
                .unwrap(),
        );
        assert!(!next_page(&current, &headers, 100).unwrap());
    }

    #[test]
    fn retry_after_dates_seconds_and_reset_are_bounded() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "0".parse().unwrap());
        assert!(retry_delay(&headers, 0, true).unwrap() < Duration::from_secs(1));
        let date = (OffsetDateTime::now_utc() + time::Duration::seconds(3))
            .format(&Rfc2822)
            .unwrap();
        headers.insert("retry-after", date.parse().unwrap());
        assert!(retry_delay(&headers, 0, true).unwrap() >= Duration::from_secs(2));
        headers.insert("retry-after", "99999".parse().unwrap());
        assert!(retry_delay(&headers, 0, true).is_err());
        headers.insert("retry-after", "0".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());
        headers.insert(
            "x-ratelimit-reset",
            (OffsetDateTime::now_utc().unix_timestamp() + 120)
                .to_string()
                .parse()
                .unwrap(),
        );
        assert!(retry_delay(&headers, 0, true).is_err());
    }

    #[tokio::test]
    async fn full_pages_continue_without_link_and_partial_failure_preserves_records() {
        for fail in [false, true] {
            let (origin, task) = mock(move |target| {
                if target.ends_with("page=1") {
                    (
                        200,
                        String::new(),
                        serde_json::to_string(&vec![serde_json::json!({"id":1}); 100]).unwrap(),
                    )
                } else if fail {
                    (403, String::new(), "do-not-leak".into())
                } else {
                    (200, String::new(), "[{\"id\":2}]".into())
                }
            })
            .await;
            let result = provider(&origin)
                .list(&["items"], &[], &mut Budget::default())
                .await;
            assert_eq!(result.items.len(), if fail { 100 } else { 101 });
            assert_eq!(result.error.is_some(), fail);
            task.abort();
        }
    }

    #[tokio::test]
    async fn body_rows_and_request_budgets_are_enforced() {
        let (origin, task) = mock(|_| (200, String::new(), " ".repeat(MAX_BODY + 1))).await;
        let result = provider(&origin)
            .list(&["items"], &[], &mut Budget::default())
            .await;
        assert_eq!(result.error.unwrap().code, "limit");
        task.abort();
        let (origin, task) = mock(|_| (200, String::new(), "[{}, {}, {}]".into())).await;
        let mut budget = Budget {
            rows: MAX_ROWS - 1,
            requests: 0,
        };
        let result = provider(&origin).list(&["items"], &[], &mut budget).await;
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.error.unwrap().code, "limit");
        let mut budget = Budget {
            rows: 0,
            requests: MAX_REQUESTS,
        };
        let result = provider(&origin).list(&["items"], &[], &mut budget).await;
        assert_eq!(result.error.unwrap().code, "limit");
        task.abort();
    }

    #[tokio::test]
    async fn retries_403_with_rate_headers_and_caps_5xx_attempts() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        for status in [403, 503] {
            let attempts = Arc::new(AtomicUsize::new(0));
            let counter = attempts.clone();
            let (origin, task) = mock(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                (status, "Retry-After: 0\r\n".into(), "secret".into())
            })
            .await;
            let result = provider(&origin)
                .list(&["items"], &[], &mut Budget::default())
                .await;
            assert!(result.error.is_some());
            assert_eq!(attempts.load(Ordering::SeqCst), 3);
            task.abort();
        }
    }

    #[tokio::test]
    async fn health_only_checks_user_and_active_membership() {
        let (origin, task) = mock(|target| {
            let body = match target {
                "/user" => r#"{"id":1,"login":"alice"}"#,
                "/user/memberships/orgs/acme" => r#"{"state":"active"}"#,
                _ => panic!("health must not enumerate graph"),
            };
            (200, String::new(), body.into())
        })
        .await;
        assert!(provider(&origin).check().await.is_ok());
        task.abort();
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let (origin, task) = mock(|_| {
            (
                302,
                "Location: https://evil.invalid/token\r\n".into(),
                String::new(),
            )
        })
        .await;
        let result = provider(&origin)
            .list(&["items"], &[], &mut Budget::default())
            .await;
        assert_eq!(result.error.unwrap().code, "http");
        task.abort();
    }

    #[tokio::test]
    async fn secondary_rate_limit_body_without_headers_waits_and_can_be_cancelled() {
        let (origin, task) = mock(|_| {
            (
                403,
                String::new(),
                r#"{"message":"You have exceeded a secondary rate limit. private-test-token"}"#
                    .into(),
            )
        })
        .await;
        let p = provider(&origin);
        let mut budget = Budget::default();
        let pending = tokio::time::timeout(
            Duration::from_millis(100),
            p.list(&["items"], &[], &mut budget),
        )
        .await;
        assert!(
            pending.is_err(),
            "secondary rate limits must wait before retry"
        );
        assert_eq!(budget.requests, 1);
        task.abort();
    }

    #[tokio::test]
    async fn repeated_full_pages_stop_at_page_cap() {
        let (origin, task) = mock(|_| {
            (
                200,
                String::new(),
                serde_json::to_string(&vec![serde_json::json!({"id":1}); 100]).unwrap(),
            )
        })
        .await;
        let mut budget = Budget::default();
        let result = provider(&origin).list(&["items"], &[], &mut budget).await;
        assert_eq!(result.items.len(), MAX_PAGES * 100);
        assert_eq!(budget.requests, MAX_PAGES);
        assert_eq!(result.error.unwrap().code, "limit");
        task.abort();
    }

    #[tokio::test]
    async fn malformed_record_and_failed_team_role_keep_valid_native_records() {
        let (origin, task) = mock(|target| {
            let body = match target {
                "/orgs/acme" => r#"{"id":9,"login":"acme"}"#,
                "/orgs/acme/members?role=member&per_page=100&page=1" => {
                    r#"[{"id":1,"login":"alice"},{"login":"missing-id"}]"#
                }
                "/orgs/acme/teams?per_page=100&page=1" => {
                    r#"[{"id":3,"slug":"eng","name":"Engineering"}]"#
                }
                "/orgs/acme/teams/eng/repos?per_page=100&page=1" => {
                    r#"[{"id":7,"full_name":"acme/app","permissions":{"admin":true}}]"#
                }
                "/orgs/acme/teams/eng/repos/acme/app" => {
                    return (403, String::new(), "private-test-token".into());
                }
                _ => "[]",
            };
            (200, String::new(), body.into())
        })
        .await;
        let snapshot = provider(&origin).discover().await.unwrap();
        snapshot.validate().unwrap();
        assert!(!snapshot.complete);
        assert_eq!(snapshot.accounts.len(), 1);
        assert_eq!(snapshot.accounts[0].key.id, "1");
        assert!(
            snapshot
                .grants
                .iter()
                .any(|g| g.role == "unknown" && g.privilege == Privilege::Unknown)
        );
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("private-test-token")
        );
        task.abort();
    }
}
