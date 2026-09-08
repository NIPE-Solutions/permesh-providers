// SPDX-License-Identifier: MIT OR Apache-2.0
//! Minimal API field validation and deduplicated domain record construction.
use crate::client::List;
use crate::{VISIBILITY, error};
use permesh_core::*;
use permesh_provider_sdk::{ProviderError, validate_snapshot};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
type GrantKey = (String, Subject, EntityKey);
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub(super) struct Collection {
    pub(super) snapshot: Snapshot,
    pub(super) accounts: BTreeMap<EntityKey, Account>,
    pub(super) resources: BTreeMap<EntityKey, Resource>,
    pub(super) groups: BTreeMap<EntityKey, Group>,
    pub(super) memberships: BTreeMap<(Subject, EntityKey), Membership>,
    pub(super) grants: BTreeMap<GrantKey, Grant>,
    account_types: BTreeMap<EntityKey, String>,
    teams: BTreeMap<EntityKey, (String, String, String)>,
    blocked_accounts: BTreeSet<EntityKey>,
    blocked_resources: BTreeSet<EntityKey>,
    blocked_groups: BTreeSet<EntityKey>,
    blocked_grants: BTreeSet<GrantKey>,
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
            account_types: BTreeMap::new(),
            teams: BTreeMap::new(),
            blocked_accounts: BTreeSet::new(),
            blocked_resources: BTreeSet::new(),
            blocked_groups: BTreeSet::new(),
            blocked_grants: BTreeSet::new(),
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
        let Some(id) = native_id(value) else {
            self.fail("account record", error("malformed"));
            return None;
        };
        let key = EntityKey::new(&self.snapshot.provider, id.to_string());
        if self.blocked_accounts.contains(&key) {
            return None;
        }
        let Some(login) = field(value, "login") else {
            self.blocked_accounts.insert(key.clone());
            self.accounts.remove(&key);
            self.fail("account record", error("malformed"));
            return None;
        };
        let native_type = field(value, "type").map(str::to_owned);
        if value.get("type").is_some() && native_type.is_none() {
            self.blocked_accounts.insert(key.clone());
            self.accounts.remove(&key);
            self.fail("account record", error("malformed"));
            return None;
        }
        if self
            .accounts
            .get(&key)
            .is_some_and(|old| old.login != login)
            || self
                .account_types
                .get(&key)
                .zip(native_type.as_ref())
                .is_some_and(|(old, new)| old != new)
        {
            self.blocked_accounts.insert(key.clone());
            self.accounts.remove(&key);
            self.fail("account record", error("conflict"));
            return None;
        }
        let kind = match native_type.as_deref() {
            Some("User") => IdentityKind::Human,
            Some("Bot") => IdentityKind::Bot,
            _ => IdentityKind::Unknown,
        };
        let account = self.accounts.entry(key.clone()).or_insert_with(|| Account {
            key: key.clone(),
            login: login.into(),
            kind,
            verified_emails: vec![],
        });
        // An omitted type is not a contrary claim. Refine unknown observations
        // when a type arrives, and preserve that evidence across later omissions.
        if let Some(native_type) = native_type {
            account.kind = kind;
            self.account_types.insert(key.clone(), native_type);
        }
        Some(key)
    }
    pub(super) fn repository(&mut self, value: &Value) -> Option<(EntityKey, String, String)> {
        let Some(id) = native_id(value) else {
            self.fail("repository record", error("malformed"));
            return None;
        };
        let key = self.key("repository", id);
        if self.blocked_resources.contains(&key) {
            return None;
        }
        let parsed = field(value, "full_name").and_then(|full_name| {
            full_name
                .split_once('/')
                .filter(|(owner, name)| safe_segment(owner) && safe_segment(name))
                .map(|(owner, name)| (full_name, owner, name))
        });
        let Some((full_name, owner, name)) = parsed else {
            self.blocked_resources.insert(key.clone());
            self.resources.remove(&key);
            self.fail("repository record", error("malformed"));
            return None;
        };
        if self
            .resources
            .get(&key)
            .is_some_and(|old| old.name != full_name)
        {
            self.blocked_resources.insert(key.clone());
            self.resources.remove(&key);
            self.fail("repository record", error("conflict"));
            return None;
        }
        self.resources
            .entry(key.clone())
            .or_insert_with(|| Resource {
                key: key.clone(),
                name: full_name.into(),
            });
        Some((key, owner.into(), name.into()))
    }
    /// Identical team repeats need no second traversal. Conflicts remain blocked
    /// for this collection even if a later row repeats the original metadata.
    pub(super) fn team(&mut self, value: &Value, org: &str) -> Option<(EntityKey, String)> {
        let Some(id) = native_id(value) else {
            self.fail("teams", error("malformed"));
            return None;
        };
        let key = self.key("team", id);
        if self.blocked_groups.contains(&key) {
            return None;
        }
        let parsed = field(value, "slug")
            .filter(|s| safe_segment(s))
            .zip(field(value, "name"));
        let Some((slug, name)) = parsed else {
            self.blocked_groups.insert(key.clone());
            self.groups.remove(&key);
            self.fail("teams", error("malformed"));
            return None;
        };
        let metadata = (org.to_owned(), slug.to_owned(), name.to_owned());
        if let Some(old) = self.teams.get(&key) {
            if old != &metadata {
                self.blocked_groups.insert(key.clone());
                self.groups.remove(&key);
                self.fail("teams", error("conflict"));
            }
            return None;
        }
        self.teams.insert(key.clone(), metadata);
        self.groups.insert(
            key.clone(),
            Group {
                key: key.clone(),
                name: format!("{org}/{name}"),
            },
        );
        Some((key, slug.into()))
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
        let claim = (method.to_owned(), subject.clone(), resource.clone());
        if self.blocked_grants.contains(&claim) {
            return;
        }
        if let Some(old) = self.grants.get(&claim) {
            if old.role != role || old.privilege != privilege {
                self.grants.remove(&claim);
                self.blocked_grants.insert(claim);
                self.fail("grant record", error("conflict"));
            }
            return;
        }
        let (kind, key) = match &subject {
            Subject::Account(k) => ("account", k),
            Subject::Group(k) => ("group", k),
        };
        let id = format!("{method}:{kind}:{}:{}:{role}", key.id, resource.id);
        self.grants.insert(
            claim,
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
        let valid_subject = |subject: &Subject| match subject {
            Subject::Account(key) => !self.blocked_accounts.contains(key),
            Subject::Group(key) => !self.blocked_groups.contains(key),
        };
        self.memberships.retain(|_, membership| {
            valid_subject(&membership.member) && !self.blocked_groups.contains(&membership.group)
        });
        self.grants.retain(|_, grant| {
            valid_subject(&grant.subject) && !self.blocked_resources.contains(&grant.resource)
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_account_type_refines_to_or_preserves_known_kind()
    -> Result<(), Box<dyn std::error::Error>> {
        for types in [[None, Some("User")], [Some("User"), None]] {
            let mut state = Collection::new("github")?;
            state.success = true;
            for native_type in types {
                let mut row = serde_json::json!({"id":1,"login":"alice"});
                if let Some(native_type) = native_type {
                    row["type"] = serde_json::json!(native_type);
                }
                assert!(state.account(&row).is_some(), "{types:?}");
            }
            let snapshot = state.finish()?;
            snapshot.validate()?;
            assert!(snapshot.complete);
            assert_eq!(snapshot.accounts.len(), 1);
            assert_eq!(snapshot.accounts[0].kind, IdentityKind::Human);
        }
        Ok(())
    }
    #[test]
    fn incompatible_known_types_stay_blocked_across_later_omissions()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut state = Collection::new("github")?;
        state.success = true;
        for (index, native_type) in [Some("User"), Some("Bot"), None, Some("User")]
            .into_iter()
            .enumerate()
        {
            let mut row = serde_json::json!({"id":1,"login":"alice"});
            if let Some(native_type) = native_type {
                row["type"] = serde_json::json!(native_type);
            }
            assert_eq!(state.account(&row).is_some(), index == 0);
        }
        let snapshot = state.finish()?;
        snapshot.validate()?;
        assert!(!snapshot.complete);
        assert!(snapshot.accounts.is_empty());
        Ok(())
    }
}
