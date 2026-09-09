// SPDX-License-Identifier: MIT
//! Explicit outbound projection onto the independent negotiated-v1 DTOs.
use permesh_core as domain;
use permesh_provider_protocol::negotiated::records as wire;
use permesh_provider_sdk as sdk;

pub(super) fn capability(value: sdk::Capability) -> wire::Capability {
    match value {
        sdk::Capability::Accounts => wire::Capability::Accounts,
        sdk::Capability::Identities => wire::Capability::Identities,
        sdk::Capability::Resources => wire::Capability::Resources,
        sdk::Capability::Groups => wire::Capability::Groups,
        sdk::Capability::Memberships => wire::Capability::Memberships,
        sdk::Capability::Grants => wire::Capability::Grants,
    }
}
fn identity_kind(value: domain::IdentityKind) -> wire::IdentityKind {
    match value {
        domain::IdentityKind::Human => wire::IdentityKind::Human,
        domain::IdentityKind::Service => wire::IdentityKind::Service,
        domain::IdentityKind::Bot => wire::IdentityKind::Bot,
        domain::IdentityKind::Unknown => wire::IdentityKind::Unknown,
    }
}
fn identity_status(value: domain::IdentityStatus) -> wire::IdentityStatus {
    match value {
        domain::IdentityStatus::Active => wire::IdentityStatus::Active,
        domain::IdentityStatus::Inactive => wire::IdentityStatus::Inactive,
        domain::IdentityStatus::Suspended => wire::IdentityStatus::Suspended,
        domain::IdentityStatus::Unknown => wire::IdentityStatus::Unknown,
    }
}
fn affiliation(value: domain::Affiliation) -> wire::Affiliation {
    match value {
        domain::Affiliation::Internal => wire::Affiliation::Internal,
        domain::Affiliation::External => wire::Affiliation::External,
        domain::Affiliation::Unknown => wire::Affiliation::Unknown,
    }
}
fn evidence_kind(value: domain::EvidenceKind) -> wire::EvidenceKind {
    match value {
        domain::EvidenceKind::Permission => wire::EvidenceKind::Permission,
        domain::EvidenceKind::Assignment => wire::EvidenceKind::Assignment,
        domain::EvidenceKind::PolicyAttachment => wire::EvidenceKind::PolicyAttachment,
        domain::EvidenceKind::Unknown => wire::EvidenceKind::Unknown,
    }
}
fn privilege(value: domain::Privilege) -> wire::Privilege {
    match value {
        domain::Privilege::Standard => wire::Privilege::Standard,
        domain::Privilege::Elevated => wire::Privilege::Elevated,
        domain::Privilege::Admin => wire::Privilege::Admin,
        domain::Privilege::Owner => wire::Privilege::Owner,
        domain::Privilege::Unknown => wire::Privilege::Unknown,
    }
}
fn certainty(value: domain::Certainty) -> wire::Certainty {
    match value {
        domain::Certainty::Observed => wire::Certainty::Observed,
        domain::Certainty::Derived => wire::Certainty::Derived,
        domain::Certainty::Inferred => wire::Certainty::Inferred,
        domain::Certainty::Unknown => wire::Certainty::Unknown,
    }
}
fn key(value: &domain::EntityKey) -> wire::EntityKey {
    wire::EntityKey {
        provider: value.provider.clone(),
        id: value.id.clone(),
    }
}
fn subject(value: &domain::Subject) -> wire::Subject {
    match value {
        domain::Subject::Account(value) => wire::Subject::Account(key(value)),
        domain::Subject::Group(value) => wire::Subject::Group(key(value)),
    }
}
fn provenance(value: &domain::Provenance) -> wire::Provenance {
    wire::Provenance {
        method: value.method.clone(),
        observed_at: value.observed_at.clone(),
    }
}
pub(super) fn account(value: &domain::Account) -> wire::Record {
    wire::Record::Account(wire::Account {
        key: key(&value.key),
        login: value.login.clone(),
        status: identity_status(value.status),
        kind: identity_kind(value.kind),
        affiliation: affiliation(value.affiliation),
        verified_emails: value.verified_emails.clone(),
    })
}
pub(super) fn identity(value: &domain::Identity) -> wire::Record {
    wire::Record::Identity(wire::Identity {
        id: value.id.clone(),
        kind: identity_kind(value.kind),
        affiliation: affiliation(value.affiliation),
        status: identity_status(value.status),
        verified_emails: value.verified_emails.clone(),
    })
}
pub(super) fn resource(value: &domain::Resource) -> wire::Record {
    wire::Record::Resource(wire::Resource {
        key: key(&value.key),
        name: value.name.clone(),
        kind: value.kind.clone(),
        parent: value.parent.as_ref().map(key),
    })
}
pub(super) fn group(value: &domain::Group) -> wire::Record {
    wire::Record::Group(wire::Group {
        key: key(&value.key),
        name: value.name.clone(),
    })
}
pub(super) fn membership(value: &domain::Membership) -> wire::Record {
    wire::Record::Membership(wire::Membership {
        member: subject(&value.member),
        group: key(&value.group),
        provenance: provenance(&value.provenance),
    })
}
pub(super) fn grant(value: &domain::Grant) -> wire::Record {
    wire::Record::Grant(wire::Grant {
        id: value.id.clone(),
        subject: subject(&value.subject),
        resource: key(&value.resource),
        role: value.role.clone(),
        privilege: privilege(value.privilege),
        certainty: certainty(value.certainty),
        evidence_kind: evidence_kind(value.evidence_kind),
        provenance: provenance(&value.provenance),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn domain_enum_values_keep_their_negotiated_wire_meanings() -> Result<(), serde_json::Error> {
        use domain::{Certainty as C, IdentityKind as K, IdentityStatus as S, Privilege as P};
        assert_eq!(
            serde_json::to_value([K::Human, K::Service, K::Bot, K::Unknown].map(identity_kind))?,
            serde_json::json!(["human", "service", "bot", "unknown"])
        );
        assert_eq!(
            serde_json::to_value(
                [S::Active, S::Inactive, S::Suspended, S::Unknown].map(identity_status)
            )?,
            serde_json::json!(["active", "inactive", "suspended", "unknown"])
        );
        assert_eq!(
            serde_json::to_value(
                [P::Standard, P::Elevated, P::Admin, P::Owner, P::Unknown].map(privilege)
            )?,
            serde_json::json!(["standard", "elevated", "admin", "owner", "unknown"])
        );
        assert_eq!(
            serde_json::to_value(
                [C::Observed, C::Derived, C::Inferred, C::Unknown].map(certainty)
            )?,
            serde_json::json!(["observed", "derived", "inferred", "unknown"])
        );
        assert_eq!(
            serde_json::to_value(
                [
                    domain::Affiliation::Internal,
                    domain::Affiliation::External,
                    domain::Affiliation::Unknown
                ]
                .map(affiliation)
            )?,
            serde_json::json!(["internal", "external", "unknown"])
        );
        assert_eq!(
            serde_json::to_value(
                [
                    domain::EvidenceKind::Permission,
                    domain::EvidenceKind::Assignment,
                    domain::EvidenceKind::PolicyAttachment,
                    domain::EvidenceKind::Unknown
                ]
                .map(evidence_kind)
            )?,
            serde_json::json!(["permission", "assignment", "policy_attachment", "unknown"])
        );
        Ok(())
    }
}
