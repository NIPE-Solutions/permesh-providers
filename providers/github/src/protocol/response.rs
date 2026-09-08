// SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Failure, io::Output};
use crate::{GithubProvider, VISIBILITY, error};
use permesh_provider_sdk::Provider;
use serde::Serialize;
use tokio::io::AsyncWrite;

pub(super) fn error_code(code: &str) -> &'static str {
    match code {
        "configuration" => "protocol_error",
        "unauthorized" => "authentication",
        "forbidden" | "not_found" | "membership" => "permission_denied",
        "rate_limit" => "rate_limited",
        "transport" | "timeout" | "limit" | "discovery" | "unavailable" => "unavailable",
        _ => "internal",
    }
}
fn limitations(values: &[String]) -> Vec<&'static str> {
    let mut codes = Vec::new();
    for value in values {
        let code = if VISIBILITY.contains(&value.as_str()) {
            "visibility_limited"
        } else if value.ends_with(&error("forbidden").message)
            || value.ends_with(&error("not_found").message)
        {
            "permission_denied"
        } else if value.ends_with(&error("rate_limit").message) {
            "rate_limited"
        } else if value.ends_with(&error("limit").message) {
            "page_limit"
        } else {
            "unknown"
        };
        if !codes.contains(&code) {
            codes.push(code);
        }
    }
    codes
}
#[derive(Serialize)]
struct Record<'a, T> {
    event: &'static str,
    kind: &'static str,
    data: &'a T,
}
async fn records<W: AsyncWrite + Unpin, T: Serialize>(
    out: &mut Output<W>,
    kind: &'static str,
    values: &[T],
) -> Result<(), Failure> {
    for data in values {
        out.send(&Record {
            event: "record",
            kind,
            data,
        })
        .await?;
    }
    Ok(())
}
pub(super) async fn operation<W: AsyncWrite + Unpin>(
    provider: &GithubProvider,
    check: bool,
    out: &mut Output<W>,
) -> Result<(), Failure> {
    if check {
        let health = provider
            .check()
            .await
            .map_err(|e| Failure::Provider(error_code(&e.code)))?;
        out.send(&serde_json::json!({"event":"health","status":"ok","limitations":limitations(&health.limitations)})).await
    } else {
        let snapshot = provider
            .discover()
            .await
            .map_err(|e| Failure::Provider(error_code(&e.code)))?;
        let count = snapshot.accounts.len()
            + snapshot.resources.len()
            + snapshot.groups.len()
            + snapshot.memberships.len()
            + snapshot.grants.len();
        if count > 100_000 {
            return Err(Failure::Internal);
        }
        records(out, "account", &snapshot.accounts).await?;
        records(out, "resource", &snapshot.resources).await?;
        records(out, "group", &snapshot.groups).await?;
        records(out, "membership", &snapshot.memberships).await?;
        records(out, "grant", &snapshot.grants).await?;
        out.send(&serde_json::json!({"event":"complete","count":count,"complete":snapshot.complete,"limitations":limitations(&snapshot.limitations)})).await
    }
}
