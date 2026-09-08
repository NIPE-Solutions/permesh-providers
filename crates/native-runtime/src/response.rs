// SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Failure, io::Output};
use crate::Adapter;
use permesh_provider_sdk::Provider;
use serde::Serialize;
use tokio::io::AsyncWrite;

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
pub(super) async fn operation<A: Adapter, P: Provider, W: AsyncWrite + Unpin>(
    provider: &P,
    check: bool,
    out: &mut Output<W>,
) -> Result<(), Failure> {
    if check {
        let health = provider
            .check()
            .await
            .map_err(|e| Failure::Provider(A::error_code(&e.code)))?;
        out.send(&serde_json::json!({"event":"health","status":"ok","limitations":A::limitations(&health.limitations)})).await
    } else {
        let snapshot = provider
            .discover()
            .await
            .map_err(|e| Failure::Provider(A::error_code(&e.code)))?;
        let count = snapshot.identities.len()
            + snapshot.accounts.len()
            + snapshot.resources.len()
            + snapshot.groups.len()
            + snapshot.memberships.len()
            + snapshot.grants.len();
        if count > 100_000 {
            return Err(Failure::Internal);
        }
        records(out, "account", &snapshot.accounts).await?;
        records(out, "identity", &snapshot.identities).await?;
        records(out, "resource", &snapshot.resources).await?;
        records(out, "group", &snapshot.groups).await?;
        records(out, "membership", &snapshot.memberships).await?;
        records(out, "grant", &snapshot.grants).await?;
        out.send(&serde_json::json!({"event":"complete","count":count,"complete":snapshot.complete,"limitations":A::limitations(&snapshot.limitations)})).await
    }
}
