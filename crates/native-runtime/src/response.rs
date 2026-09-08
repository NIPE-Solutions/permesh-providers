// SPDX-License-Identifier: MIT OR Apache-2.0
use super::{Failure, io::Output};
use crate::Adapter;
use permesh_provider_sdk::Provider;
use serde::Serialize;
use tokio::io::AsyncWrite;

#[derive(Serialize)]
struct Record {
    event: &'static str,
    #[serde(flatten)]
    record: permesh_provider_protocol::records::Record,
}
async fn records<W: AsyncWrite + Unpin, T>(
    out: &mut Output<W>,
    values: &[T],
    convert: fn(&T) -> permesh_provider_protocol::records::Record,
) -> Result<(), Failure> {
    for data in values {
        out.send(&Record {
            event: "record",
            record: convert(data),
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
        records(out, &snapshot.accounts, crate::records::account).await?;
        records(out, &snapshot.identities, crate::records::identity).await?;
        records(out, &snapshot.resources, crate::records::resource).await?;
        records(out, &snapshot.groups, crate::records::group).await?;
        records(out, &snapshot.memberships, crate::records::membership).await?;
        records(out, &snapshot.grants, crate::records::grant).await?;
        out.send(&serde_json::json!({"event":"complete","count":count,"complete":snapshot.complete,"limitations":A::limitations(&snapshot.limitations)})).await
    }
}
