// SPDX-License-Identifier: MIT
use permesh_core::*;
use permesh_native_runtime::{Adapter, serve};
use permesh_provider_protocol::DiscoveryDecoder;
use permesh_provider_sdk::{
    Capability, Health, Metadata, Provider, ProviderFuture, setup::SetupSpec,
};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

struct Fixture(Snapshot);
impl Adapter for Fixture {
    type Configuration = Value;
    type Credentials = Value;
    fn metadata() -> Metadata {
        Metadata {
            kind: "fixture".into(),
            capabilities: vec![
                Capability::Accounts,
                Capability::Identities,
                Capability::Resources,
                Capability::Groups,
                Capability::Memberships,
                Capability::Grants,
            ],
        }
    }
    fn setup() -> SetupSpec {
        panic!("discovery must not request setup")
    }
    fn validate(configuration: &Value, credentials: &Value) -> bool {
        configuration == &serde_json::json!({}) && credentials == &serde_json::json!({})
    }
    fn error_code(_: &str) -> &'static str {
        "internal"
    }
    fn limitations(_: &[String]) -> Vec<&'static str> {
        vec![]
    }
}
impl Provider for Fixture {
    fn metadata(&self) -> Metadata {
        <Self as Adapter>::metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        panic!("discovery must not check")
    }
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        Box::pin(async { Ok(self.0.clone()) })
    }
}
fn fixture() -> Snapshot {
    let key = |id: &str| EntityKey::new("test", id);
    let provenance = |method: &str| Provenance {
        method: method.into(),
        observed_at: "2026-01-01T00:00:00Z".into(),
    };
    let mut snapshot = Snapshot::new("test");
    snapshot.accounts.push(Account {
        key: key("account"),
        login: "alice".into(),
        kind: IdentityKind::Human,
        verified_emails: vec!["alice@example.com".into()],
    });
    snapshot.identities.push(Identity {
        id: "directory:alice".into(),
        kind: IdentityKind::Unknown,
        status: IdentityStatus::Active,
        verified_emails: vec!["alice@example.com".into()],
    });
    snapshot.resources.push(Resource {
        key: key("resource"),
        name: "Repository".into(),
    });
    snapshot.groups.push(Group {
        key: key("group"),
        name: "Engineering".into(),
    });
    snapshot.memberships.push(Membership {
        member: Subject::Account(key("account")),
        group: key("group"),
        provenance: provenance("fixture.membership"),
    });
    snapshot.grants.push(Grant {
        id: "grant".into(),
        subject: Subject::Group(key("group")),
        resource: key("resource"),
        role: "write".into(),
        privilege: Privilege::Elevated,
        certainty: Certainty::Observed,
        provenance: provenance("fixture.grant"),
    });
    snapshot
}
#[tokio::test]
async fn native_discovery_preserves_frozen_bytes_and_host_decodes_all_record_kinds()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut input, reader) = tokio::io::duplex(4096);
    input.write_all(b"{\"protocol\":2,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\"}\n{\"protocol\":2,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{},\"credentials\":{}}\n").await?;
    let mut output = Vec::new();
    serve::<Fixture, _, _, _, _, _>(reader, &mut output, |_, _, _| async {
        Ok(Fixture(fixture()))
    })
    .await?;
    assert_eq!(output, include_bytes!("fixtures/discovery.ndjson"));
    let capabilities = <Fixture as Adapter>::metadata().capabilities;
    let mut decoder = DiscoveryDecoder::new_versioned("fixture", "test", Some(&capabilities), 2)?;
    for frame in output.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame)?;
    }
    let snapshot = decoder.finish()?;
    assert!(snapshot.complete);
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(snapshot.identities.len(), 1);
    assert_eq!(snapshot.resources.len(), 1);
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(snapshot.memberships.len(), 1);
    assert_eq!(snapshot.grants.len(), 1);
    Ok(())
}
