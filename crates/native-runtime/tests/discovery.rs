// SPDX-License-Identifier: MIT
use permesh_core::*;
use permesh_native_runtime::{Adapter, serve};
use permesh_provider_protocol::negotiated::DiscoveryDecoder;
use permesh_provider_sdk::{
    Capability, Health, Metadata, Provider, ProviderFuture, setup::SetupSpec,
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
use tokio::io::AsyncWriteExt;

struct Fixture(Snapshot);
impl Adapter for Fixture {
    type Configuration = Empty;
    type Credentials = Empty;
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
    fn validate(_: &Empty, _: &Empty) -> bool {
        true
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
        kind: IdentityKind::Service,
        affiliation: Affiliation::External,
        status: IdentityStatus::Suspended,
        verified_emails: vec!["alice@example.com".into()],
    });
    snapshot.identities.push(Identity {
        id: "directory:alice".into(),
        kind: IdentityKind::Service,
        affiliation: Affiliation::Internal,
        status: IdentityStatus::Active,
        verified_emails: vec!["alice@example.com".into()],
    });
    snapshot.resources.push(Resource {
        key: key("resource"),
        name: "Repository".into(),
        kind: Some("fixture.repository".into()),
        parent: Some(key("root")),
    });
    snapshot.resources.push(Resource {
        key: key("root"),
        name: "Root".into(),
        kind: None,
        parent: None,
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
        certainty: Certainty::Derived,
        evidence_kind: EvidenceKind::PolicyAttachment,
        provenance: provenance("fixture.grant"),
    });
    snapshot.resources.reverse();
    snapshot
}
#[tokio::test]
async fn native_discovery_preserves_rich_bytes_and_host_decodes_all_record_kinds()
-> Result<(), Box<dyn std::error::Error>> {
    let (mut input, reader) = tokio::io::duplex(4096);
    input.write_all(b"{\"protocol_version\":1,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\",\"operation\":\"discover\"}\n{\"protocol_version\":1,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{},\"credentials\":{}}\n").await?;
    let mut output = Vec::new();
    serve::<Fixture, _, _, _, _, _>(reader, &mut output, |_, _, _| async {
        Ok(Fixture(fixture()))
    })
    .await?;
    assert_eq!(
        output,
        include_bytes!("fixtures/negotiated-discovery.ndjson")
    );
    let capabilities = <Fixture as Adapter>::metadata().capabilities;
    let mut decoder = DiscoveryDecoder::new("fixture", "test", Some(&capabilities))?;
    for frame in output.split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame)?;
    }
    let snapshot = decoder.finish()?;
    assert!(snapshot.complete);
    assert_eq!(snapshot.accounts[0].status, IdentityStatus::Suspended);
    assert_eq!(snapshot.accounts[0].affiliation, Affiliation::External);
    assert_eq!(snapshot.identities[0].kind, IdentityKind::Service);
    assert_eq!(snapshot.identities[0].status, IdentityStatus::Active);
    assert_eq!(snapshot.grants[0].certainty, Certainty::Derived);
    assert_eq!(
        snapshot.grants[0].evidence_kind,
        EvidenceKind::PolicyAttachment
    );
    assert_eq!(
        snapshot.resources[0].parent,
        Some(EntityKey::new("test", "root"))
    );
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(snapshot.identities.len(), 1);
    assert_eq!(snapshot.resources.len(), 2);
    assert_eq!(snapshot.groups.len(), 1);
    assert_eq!(snapshot.memberships.len(), 1);
    assert_eq!(snapshot.grants.len(), 1);
    Ok(())
}

#[test]
fn historical_draft_two_literal_remains_decodable() -> Result<(), Box<dyn std::error::Error>> {
    let capabilities = <Fixture as Adapter>::metadata().capabilities;
    let mut decoder = permesh_provider_protocol::DiscoveryDecoder::new_versioned(
        "fixture",
        "test",
        Some(&capabilities),
        2,
    )?;
    for frame in include_bytes!("fixtures/discovery.ndjson").split_inclusive(|b| *b == b'\n') {
        decoder.push_frame(frame)?;
    }
    assert_eq!(decoder.finish()?.accounts.len(), 1);
    Ok(())
}

#[tokio::test]
async fn whole_snapshot_is_validated_before_any_record_is_emitted()
-> Result<(), Box<dyn std::error::Error>> {
    for mutation in 0..6 {
        let mut snapshot = fixture();
        match mutation {
            0 => snapshot.provider = "other".into(),
            1 => snapshot.resources[0].parent = Some(EntityKey::new("test", "missing")),
            2 => snapshot.resources[0].parent = Some(snapshot.resources[1].key.clone()),
            3 => snapshot.grants[0].provenance.observed_at = "private-invalid-timestamp".into(),
            4 => snapshot.resources[0].kind = Some("invalid kind".into()),
            _ => snapshot.accounts.push(snapshot.accounts[0].clone()),
        }
        let (mut input, reader) = tokio::io::duplex(4096);
        input.write_all(b"{\"protocol_version\":1,\"id\":\"handshake\",\"method\":\"handshake\",\"instance\":\"test\",\"operation\":\"discover\"}\n{\"protocol_version\":1,\"id\":\"discover\",\"method\":\"discover\",\"configuration\":{},\"credentials\":{}}\n").await?;
        let mut output = Vec::new();
        assert!(
            serve::<Fixture, _, _, _, _, _>(reader, &mut output, |_, _, _| async {
                Ok(Fixture(snapshot))
            })
            .await
            .is_err()
        );
        let frames: Vec<serde_json::Value> = output
            .split_inclusive(|b| *b == b'\n')
            .map(serde_json::from_slice)
            .collect::<Result<_, _>>()?;
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1]["event"], "error");
        assert_eq!(frames[1]["code"], "internal");
        assert!(!String::from_utf8(output)?.contains("private-invalid"));
    }
    Ok(())
}
