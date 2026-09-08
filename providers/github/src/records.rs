// SPDX-License-Identifier: MIT OR Apache-2.0
//! Minimal API field validation and deduplicated domain record construction.
use crate::client::List;
use crate::{VISIBILITY, error};
use permesh_core::*;
use permesh_provider_sdk::{ProviderError, validate_snapshot};
use serde_json::Value;
use std::collections::BTreeMap;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) struct Collection {
    pub(super) snapshot: Snapshot,
    pub(super) accounts: BTreeMap<EntityKey, Account>,
    pub(super) resources: BTreeMap<EntityKey, Resource>,
    pub(super) groups: BTreeMap<EntityKey, Group>,
    pub(super) memberships: BTreeMap<(Subject, EntityKey), Membership>,
    pub(super) grants: BTreeMap<String, Grant>,
    pub(super) observed_at: String,
    pub(super) success: bool,
}
impl Collection {
    pub(super) fn new(id: &str) -> Result<Self, ProviderError> {
        let mut snapshot = Snapshot::new(id);
        snapshot.limitations = VISIBILITY.iter().map(|v| (*v).into()).collect();
        Ok(Self {
            snapshot,
            accounts: BTreeMap::new(),
            resources: BTreeMap::new(),
            groups: BTreeMap::new(),
            memberships: BTreeMap::new(),
            grants: BTreeMap::new(),
            observed_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .map_err(|_| error("clock"))?,
            success: false,
        })
    }
    pub(super) fn key(&self, kind: &str, id: u64) -> EntityKey {
        EntityKey::new(&self.snapshot.provider, format!("{kind}:{id}"))
    }
    pub(super) fn provenance(&self, method: &str) -> Provenance {
        Provenance {
            method: method.into(),
            observed_at: self.observed_at.clone(),
        }
    }
    pub(super) fn fail(&mut self, endpoint: &str, err: ProviderError) {
        self.snapshot.complete = false;
        // Both the endpoint category and error message are controlled constants.
        let limitation = format!("GitHub {endpoint}: {}", err.message);
        if !self.snapshot.limitations.contains(&limitation) {
            self.snapshot.limitations.push(limitation);
        }
    }
    pub(super) fn accept(&mut self, endpoint: &str, list: List) -> Vec<Value> {
        self.success |= list.success;
        if let Some(e) = list.error {
            self.fail(endpoint, e);
        }
        list.items
    }
    pub(super) fn account(&mut self, value: &Value) -> Option<EntityKey> {
        let (Some(id), Some(login)) = (native_id(value), field(value, "login")) else {
            self.fail("account record", error("malformed"));
            return None;
        };
        let key = EntityKey::new(&self.snapshot.provider, id.to_string());
        self.accounts.entry(key.clone()).or_insert_with(|| Account {
            key: key.clone(),
            login: login.into(),
            kind: match field(value, "type") {
                Some("User") => IdentityKind::Human,
                Some("Bot") => IdentityKind::Bot,
                _ => IdentityKind::Unknown,
            },
            verified_emails: vec![],
        });
        Some(key)
    }
    pub(super) fn repository(&mut self, value: &Value) -> Option<(EntityKey, String, String)> {
        let (Some(id), Some(full_name)) = (native_id(value), field(value, "full_name")) else {
            self.fail("repository record", error("malformed"));
            return None;
        };
        let Some((owner, name)) = full_name.split_once('/') else {
            self.fail("repository record", error("malformed"));
            return None;
        };
        if !safe_segment(owner) || !safe_segment(name) {
            self.fail("repository record", error("malformed"));
            return None;
        }
        let key = self.key("repository", id);
        self.resources
            .entry(key.clone())
            .or_insert_with(|| Resource {
                key: key.clone(),
                name: full_name.into(),
            });
        Some((key, owner.into(), name.into()))
    }
    pub(super) fn membership(&mut self, member: Subject, group: EntityKey, method: &str) {
        let provenance = self.provenance(method);
        self.memberships
            .entry((member.clone(), group.clone()))
            .or_insert(Membership {
                member,
                group,
                provenance,
            });
    }
    pub(super) fn grant(
        &mut self,
        subject: Subject,
        resource: EntityKey,
        role: &str,
        privilege: Privilege,
        method: &str,
    ) {
        let (kind, key) = match &subject {
            Subject::Account(k) => ("account", k),
            Subject::Group(k) => ("group", k),
        };
        let id = format!("{method}:{kind}:{}:{}:{role}", key.id, resource.id);
        self.grants.insert(
            id.clone(),
            Grant {
                id,
                subject,
                resource,
                role: role.into(),
                privilege,
                certainty: Certainty::Observed,
                provenance: self.provenance(method),
            },
        );
    }
    pub(super) fn finish(mut self) -> Result<Snapshot, ProviderError> {
        if !self.success {
            return Err(error("discovery"));
        }
        self.snapshot.accounts = self.accounts.into_values().collect();
        self.snapshot.resources = self.resources.into_values().collect();
        self.snapshot.groups = self.groups.into_values().collect();
        self.snapshot.memberships = self.memberships.into_values().collect();
        self.snapshot.grants = self.grants.into_values().collect();
        self.snapshot.sort();
        validate_snapshot(&self.snapshot).map_err(|_| error("malformed"))?;
        Ok(self.snapshot)
    }
}

pub(super) fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub(super) fn native_id(v: &Value) -> Option<u64> {
    v.get("id")?.as_u64().filter(|id| *id > 0)
}
pub(super) fn field<'a>(v: &'a Value, name: &str) -> Option<&'a str> {
    v.get(name)?
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 512 && !s.chars().any(char::is_control))
}
pub(super) fn repository_role(v: &Value) -> &str {
    // A named role (including a custom role) takes precedence over boolean permissions.
    field(v, "role_name")
        .or_else(|| field(v, "permission"))
        .unwrap_or("unknown")
}
pub(super) fn privilege(role: &str) -> Privilege {
    match role {
        "read" | "pull" | "triage" => Privilege::Standard,
        "write" | "push" | "maintain" => Privilege::Elevated,
        "admin" => Privilege::Admin,
        _ => Privilege::Unknown,
    }
}
