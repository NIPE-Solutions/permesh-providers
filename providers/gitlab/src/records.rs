// SPDX-License-Identifier: MIT
use crate::{VISIBILITY, error};
use permesh_core::*;
use permesh_provider_sdk::ProviderError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn native_id(value: &Value) -> Result<u64, ProviderError> {
    value
        .get("id")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .ok_or_else(|| error("malformed"))
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, ProviderError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control))
        .ok_or_else(|| error("malformed"))
}
pub(crate) fn key(instance: &str, scope: &str, kind: &str, id: u64) -> EntityKey {
    EntityKey::new(instance, format!("{scope}:{kind}:{id}"))
}
pub(crate) fn account(
    instance: &str,
    scope: &str,
    value: &Value,
) -> Result<Account, ProviderError> {
    let id = native_id(value)?;
    let login = text(value, "username")?;
    let state = value
        .get("state")
        .map(|v| v.as_str().ok_or_else(|| error("malformed")))
        .transpose()?;
    let bot = value
        .get("bot")
        .map(|v| v.as_bool().ok_or_else(|| error("malformed")))
        .transpose()?;
    Ok(Account {
        key: key(instance, scope, "user", id),
        login: login.into(),
        kind: if bot == Some(true) {
            IdentityKind::Bot
        } else {
            IdentityKind::Unknown
        },
        affiliation: Affiliation::Unknown,
        status: match state {
            Some("active") => IdentityStatus::Active,
            Some("blocked" | "blocked_pending_approval" | "ldap_blocked") => {
                IdentityStatus::Suspended
            }
            Some("deactivated") => IdentityStatus::Inactive,
            _ => IdentityStatus::Unknown,
        },
        verified_emails: vec![],
    })
}
pub(crate) fn resource(
    instance: &str,
    scope: &str,
    kind: &str,
    id: u64,
    value: &Value,
) -> Result<Resource, ProviderError> {
    if native_id(value)? != id {
        return Err(error("scope"));
    }
    let label = text(
        value,
        if kind == "group" {
            "full_path"
        } else {
            "path_with_namespace"
        },
    )?;
    Ok(Resource {
        key: key(instance, scope, kind, id),
        name: label.into(),
        kind: Some(format!("gitlab.{kind}")),
        parent: None,
    })
}
pub(crate) fn parent(value: &Value, kind: &str) -> Result<Option<u64>, ProviderError> {
    let raw = if kind == "group" {
        value.get("parent_id")
    } else {
        let namespace = value
            .get("namespace")
            .filter(|v| v.is_object())
            .ok_or_else(|| error("malformed"))?;
        match namespace.get("kind").and_then(Value::as_str) {
            Some("group") => namespace.get("id"),
            Some("user") => return Ok(None),
            _ => return Err(error("malformed")),
        }
    };
    match raw {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| *n > 0)
            .map(Some)
            .ok_or_else(|| error("malformed")),
    }
}
pub(crate) struct Collection {
    pub snapshot: Snapshot,
    pub resources: BTreeMap<EntityKey, Resource>,
    pub groups: BTreeMap<EntityKey, Group>,
    accounts: BTreeMap<EntityKey, Account>,
    blocked: BTreeSet<EntityKey>,
    memberships: BTreeMap<(Subject, EntityKey), Membership>,
    grants: BTreeMap<String, Grant>,
    time: String,
    scope: String,
}
impl Collection {
    pub fn new(instance: &str, scope: &str) -> Result<Self, ProviderError> {
        let mut snapshot = Snapshot::new(instance);
        snapshot.limitations = VISIBILITY.iter().map(|s| (*s).into()).collect();
        Ok(Self {
            snapshot,
            resources: BTreeMap::new(),
            groups: BTreeMap::new(),
            accounts: BTreeMap::new(),
            blocked: BTreeSet::new(),
            memberships: BTreeMap::new(),
            grants: BTreeMap::new(),
            scope: scope.into(),
            time: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| error("malformed"))?,
        })
    }
    pub fn fail(&mut self, failure: ProviderError) {
        self.snapshot.complete = false;
        let message = format!("GitLab partial collection: {}", failure.code);
        if !self.snapshot.limitations.contains(&message) {
            self.snapshot.limitations.push(message);
        }
    }
    pub fn member(
        &mut self,
        resource: &EntityKey,
        group: bool,
        effective: bool,
        value: &Value,
    ) -> Result<(), ProviderError> {
        let candidate = account(&self.snapshot.provider, &self.scope, value)?;
        if self.blocked.contains(&candidate.key) {
            return Err(error("malformed"));
        }
        if self.accounts.get(&candidate.key).is_some_and(|old| {
            old.login != candidate.login
                || old.status != candidate.status
                || (old.kind != IdentityKind::Unknown
                    && candidate.kind != IdentityKind::Unknown
                    && old.kind != candidate.kind)
        }) {
            self.blocked.insert(candidate.key.clone());
            self.accounts.remove(&candidate.key);
            return Err(error("malformed"));
        }
        let level = value
            .get("access_level")
            .and_then(Value::as_u64)
            .ok_or_else(|| error("malformed"))?;
        let custom = match value.get("member_role_id") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| error("malformed"))?,
            ),
        };
        // Preserve expiry as a source qualifier rather than treating a dated record
        // as a guaranteed currently usable assignment.
        if value.get("expires_at").is_some_and(|v| !v.is_null()) {
            let expiry = text(value, "expires_at")?;
            let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
                .map_err(|_| error("malformed"))?;
            let date = time::Date::parse(expiry, &format).map_err(|_| error("malformed"))?;
            if date < time::OffsetDateTime::now_utc().date() {
                return Err(error("malformed"));
            }
        }
        let account_key = candidate.key.clone();
        self.accounts
            .entry(account_key.clone())
            .and_modify(|old| {
                if old.kind == IdentityKind::Unknown {
                    old.kind = candidate.kind
                }
            })
            .or_insert(candidate);
        let method = format!(
            "gitlab.{}.{}",
            if group { "groups" } else { "projects" },
            if effective {
                "members_all.effective_collapsed"
            } else {
                "members.direct"
            }
        );
        let provenance = Provenance {
            method,
            observed_at: self.time.clone(),
        };
        if group && !effective {
            let member = Subject::Account(account_key.clone());
            self.memberships.insert(
                (member.clone(), resource.clone()),
                Membership {
                    member,
                    group: resource.clone(),
                    provenance: provenance.clone(),
                },
            );
        }
        if level == 0 {
            return Ok(());
        }
        let name = match level {
            5 => "minimal_access",
            10 => "guest",
            15 => "planner",
            20 => "reporter",
            25 => "security_manager",
            30 => "developer",
            40 => "maintainer",
            50 => "owner",
            60 => "admin",
            _ => "unknown",
        };
        let role = format!(
            "access_level:{level}:{name}{}",
            custom
                .map(|id| format!(":member_role_id:{id}"))
                .unwrap_or_default()
        );
        // Custom roles are not evaluated. Known membership levels are native
        // privilege categories, never all effective repository permissions.
        let privilege = if custom.is_some() {
            Privilege::Unknown
        } else {
            match level {
                40 | 60 => Privilege::Admin,
                50 => Privilege::Owner,
                _ => Privilege::Unknown,
            }
        };
        let id = format!(
            "{}:{}:{}",
            if effective { "effective" } else { "direct" },
            resource.id,
            account_key.id
        );
        self.grants.insert(
            id.clone(),
            Grant {
                id,
                subject: Subject::Account(account_key),
                resource: resource.clone(),
                role,
                privilege,
                certainty: Certainty::Observed,
                evidence_kind: if effective {
                    EvidenceKind::Permission
                } else {
                    EvidenceKind::Assignment
                },
                provenance,
            },
        );
        Ok(())
    }
    pub fn finish(mut self) -> Result<Snapshot, ProviderError> {
        self.snapshot.accounts = self.accounts.into_values().collect();
        self.snapshot.resources = self.resources.into_values().collect();
        self.snapshot.groups = self.groups.into_values().collect();
        self.snapshot.memberships = self
            .memberships
            .into_values()
            .filter(|m| !matches!(&m.member,Subject::Account(key) if self.blocked.contains(key)))
            .collect();
        self.snapshot.grants = self
            .grants
            .into_values()
            .filter(|g| !matches!(&g.subject,Subject::Account(key) if self.blocked.contains(key)))
            .collect();
        self.snapshot.sort();
        self.snapshot.validate().map_err(|_| error("malformed"))?;
        if self.snapshot.accounts.len()
            + self.snapshot.resources.len()
            + self.snapshot.groups.len()
            + self.snapshot.memberships.len()
            + self.snapshot.grants.len()
            > crate::MAX_ROWS
        {
            return Err(error("limit"));
        }
        Ok(self.snapshot)
    }
}
