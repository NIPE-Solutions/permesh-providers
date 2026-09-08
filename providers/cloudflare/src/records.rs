// SPDX-License-Identifier: MIT
use super::{client::Budget, *};
use permesh_core::{
    Account, Certainty, EntityKey, Grant, Group, IdentityKind, Membership, Privilege, Provenance,
    Resource, Subject,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub(crate) fn native_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, ProviderError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control))
        .ok_or_else(|| error("malformed"))
}
fn id(v: &Value) -> Result<&str, ProviderError> {
    string(v, "id").and_then(|s| {
        if native_id(s) {
            Ok(s)
        } else {
            Err(error("malformed"))
        }
    })
}
pub(crate) fn member(v: &Value) -> Result<(&str, &str, bool), ProviderError> {
    let key = id(v)?;
    let status = string(v, "status")?;
    if !matches!(status, "accepted" | "pending") {
        return Err(error("malformed"));
    }
    let login = v
        .get("user")
        .and_then(|u| u.get("email"))
        .or_else(|| v.get("email"));
    let login = match login {
        Some(v) => v
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 254 && !s.chars().any(char::is_control))
            .ok_or_else(|| error("malformed"))?,
        None => key,
    };
    Ok((key, login, status == "accepted"))
}
fn warning(s: &mut Snapshot, code: &str) {
    s.complete = false;
    s.limitations
        .push(format!("Cloudflare partial collection: {code}."));
}
fn unique(rows: Vec<Value>, snapshot: &mut Snapshot) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let mut conflicts = BTreeSet::new();
    for row in rows {
        let key = match id(&row) {
            Ok(k) => k.to_owned(),
            Err(_) => {
                warning(snapshot, "malformed");
                continue;
            }
        };
        if conflicts.contains(&key) {
            continue;
        }
        if out.get(&key).is_some_and(|old| *old != row) {
            out.remove(&key);
            conflicts.insert(key);
            warning(snapshot, "conflicting_records");
        } else {
            out.insert(key, row);
        }
    }
    out
}
impl CloudflareProvider {
    fn key(&self, id: impl Into<String>) -> EntityKey {
        EntityKey::new(&self.id, id)
    }
    pub(crate) fn account(&self, response: &Value) -> Result<Resource, ProviderError> {
        let value = response.get("result").ok_or_else(|| error("malformed"))?;
        if id(value)? != self.account_id {
            return Err(error("scope"));
        }
        Ok(Resource {
            key: self.key(format!("account:{}", self.account_id)),
            name: string(value, "name")?.into(),
        })
    }
    pub(crate) async fn collect(&self) -> Result<Snapshot, ProviderError> {
        let mut budget = Budget::default();
        let mut snapshot = Snapshot::new(&self.id);
        snapshot.limitations = VISIBILITY.iter().map(|s| (*s).into()).collect();
        let account = self
            .request(&format!("accounts/{}", self.account_id), &[], &mut budget)
            .await?;
        snapshot.resources.push(self.account(&account)?);
        let members = self
            .listing(
                &format!("accounts/{}/members", self.account_id),
                &[],
                &mut budget,
            )
            .await;
        if let Some(code) = members.failure {
            warning(&mut snapshot, code)
        }
        let members = unique(members.rows, &mut snapshot);
        let zones = self
            .listing(
                "zones",
                &[("account.id", self.account_id.clone())],
                &mut budget,
            )
            .await;
        if let Some(code) = zones.failure {
            warning(&mut snapshot, code)
        }
        let zones = unique(zones.rows, &mut snapshot);
        let mut zone_ids = BTreeSet::new();
        for (key, zone) in zones {
            if zone
                .get("account")
                .and_then(|v| v.get("id"))
                .and_then(Value::as_str)
                != Some(&self.account_id)
            {
                warning(&mut snapshot, "scope");
                continue;
            }
            let Ok(name) = string(&zone, "name") else {
                warning(&mut snapshot, "malformed");
                continue;
            };
            zone_ids.insert(key.clone());
            snapshot.resources.push(Resource {
                key: self.key(format!("zone:{key}")),
                name: name.into(),
            });
        }
        let groups = self
            .listing(
                &format!("accounts/{}/iam/user_groups", self.account_id),
                &[],
                &mut budget,
            )
            .await;
        if let Some(code) = groups.failure {
            warning(&mut snapshot, code)
        }
        let groups = unique(groups.rows, &mut snapshot);
        let observed_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|_| error("clock"))?;
        let provenance = |method: &str| Provenance {
            method: method.into(),
            observed_at: observed_at.clone(),
        };
        let mut blocked = BTreeSet::new();
        let mut active = BTreeSet::new();
        let mut grants = Vec::new();
        for (key, value) in members {
            let Ok((_, login, accepted)) = member(&value) else {
                warning(&mut snapshot, "malformed");
                continue;
            };
            if !accepted {
                continue;
            }
            active.insert(key.clone());
            let subject = self.key(format!("member:{}:{key}", self.account_id));
            snapshot.accounts.push(Account {
                key: subject.clone(),
                login: login.into(),
                kind: IdentityKind::Unknown,
                verified_emails: vec![],
            });
            match self.policies(
                &value,
                Subject::Account(subject),
                &zone_ids,
                provenance("cloudflare.members.policy_assignment"),
            ) {
                Ok(value) => {
                    if grants.len().saturating_add(value.len()) > MAX_ROWS {
                        return Err(error("limit"));
                    }
                    grants.extend(value)
                }
                Err(_) => {
                    blocked.insert(key);
                    warning(&mut snapshot, "unsupported_policy");
                }
            }
        }
        let mut memberships = Vec::new();
        for (key, value) in groups {
            let Ok(name) = string(&value, "name") else {
                warning(&mut snapshot, "malformed");
                continue;
            };
            let group_key = self.key(format!("user-group:{}:{key}", self.account_id));
            snapshot.groups.push(Group {
                key: group_key.clone(),
                name: name.into(),
            });
            let group_grants = self.policies(
                &value,
                Subject::Group(group_key.clone()),
                &zone_ids,
                provenance("cloudflare.iam.user_groups.policy_assignment"),
            );
            let group_blocked = group_grants.is_err();
            if group_blocked {
                warning(&mut snapshot, "unsupported_policy")
            }
            if let Ok(value) = group_grants {
                if grants.len().saturating_add(value.len()) > MAX_ROWS {
                    return Err(error("limit"));
                }
                grants.extend(value)
            }
            let listed = self
                .listing(
                    &format!("accounts/{}/iam/user_groups/{key}/members", self.account_id),
                    &[],
                    &mut budget,
                )
                .await;
            if let Some(code) = listed.failure {
                warning(&mut snapshot, code)
            }
            for (member_id, row) in unique(listed.rows, &mut snapshot) {
                if row.get("status").and_then(Value::as_str) == Some("pending") {
                    continue;
                }
                if row.get("status").and_then(Value::as_str) != Some("accepted")
                    || !active.contains(&member_id)
                {
                    warning(&mut snapshot, "unresolved_membership");
                    continue;
                }
                if group_blocked {
                    blocked.insert(member_id.clone());
                    continue;
                }
                memberships.push((
                    member_id.clone(),
                    Membership {
                        member: Subject::Account(
                            self.key(format!("member:{}:{member_id}", self.account_id)),
                        ),
                        group: group_key.clone(),
                        provenance: provenance("cloudflare.iam.user_groups.members"),
                    },
                ));
            }
            if grants.len() + memberships.len() > MAX_ROWS {
                return Err(error("limit"));
            }
        }
        for (member_id, membership) in memberships {
            if !blocked.contains(&member_id) {
                snapshot.memberships.push(membership)
            }
        }
        let blocked_keys: BTreeSet<_> = blocked
            .into_iter()
            .map(|id| self.key(format!("member:{}:{id}", self.account_id)))
            .collect();
        let mut resources: BTreeMap<_, _> = snapshot
            .resources
            .drain(..)
            .map(|r| (r.key.clone(), r))
            .collect();
        let mut seen = BTreeSet::new();
        for (grant, resource) in grants {
            if matches!(&grant.subject,Subject::Account(key) if blocked_keys.contains(key)) {
                continue;
            }
            if seen.insert(grant.id.clone()) {
                snapshot.grants.push(grant);
            }
            resources.entry(resource.key.clone()).or_insert(resource);
        }
        snapshot.resources = resources.into_values().collect();
        snapshot.sort();
        permesh_provider_sdk::validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }
    fn policies(
        &self,
        value: &Value,
        subject: Subject,
        zones: &BTreeSet<String>,
        provenance: Provenance,
    ) -> Result<Vec<(Grant, Resource)>, ProviderError> {
        let Some(policies) = value.get("policies").and_then(Value::as_array) else {
            return Err(error("unsupported_policy"));
        };
        // Denies cannot be represented by the core graph. Suppress the subject's entire
        // assignment set on ambiguity, including allow entries seen before a deny.
        let mut records = Vec::new();
        let mut seen = BTreeMap::new();
        let mut role_names = BTreeMap::new();
        for policy in policies {
            let policy_id = id(policy)?;
            if policy.as_object().is_none_or(|p| {
                p.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "id" | "access" | "permission_groups" | "resource_groups"
                    )
                })
            }) {
                return Err(error("unsupported_policy"));
            }
            if seen
                .insert(policy_id, policy)
                .is_some_and(|old| old != policy)
            {
                return Err(error("unsupported_policy"));
            }
            if policy.get("access").and_then(Value::as_str) != Some("allow") {
                return Err(error("unsupported_policy"));
            }
            let roles = policy
                .get("permission_groups")
                .and_then(Value::as_array)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| error("unsupported_policy"))?;
            let resources = policy
                .get("resource_groups")
                .and_then(Value::as_array)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| error("unsupported_policy"))?;
            for resource in resources {
                if resource.as_object().is_none_or(|v| {
                    v.keys()
                        .any(|k| !matches!(k.as_str(), "id" | "scope" | "meta" | "name"))
                }) {
                    return Err(error("unsupported_policy"));
                }
                let scope = resource
                    .get("scope")
                    .ok_or_else(|| error("unsupported_policy"))?;
                if scope
                    .as_object()
                    .is_none_or(|v| v.keys().any(|k| !matches!(k.as_str(), "key" | "objects")))
                {
                    return Err(error("unsupported_policy"));
                }
                if scope.get("key").and_then(Value::as_str)
                    != Some(format!("com.cloudflare.api.account.{}", self.account_id).as_str())
                {
                    return Err(error("unsupported_policy"));
                }
                let objects = scope
                    .get("objects")
                    .and_then(Value::as_array)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| error("unsupported_policy"))?;
                for object in objects {
                    if object
                        .as_object()
                        .is_none_or(|v| v.keys().any(|k| k != "key"))
                    {
                        return Err(error("unsupported_policy"));
                    }
                    let target = string(object, "key")?;
                    let resource = if target == "*" {
                        Resource {
                            key: self.key(format!("policy-scope:account:{}:all", self.account_id)),
                            name: format!(
                                "All resources policy scope in Cloudflare account {} (assignment evidence)",
                                self.account_id
                            ),
                        }
                    } else if let Some(zone) = target
                        .strip_prefix("com.cloudflare.api.account.zone.")
                        .filter(|z| native_id(z) && zones.contains(*z))
                    {
                        Resource {
                            key: self.key(format!("zone:{zone}")),
                            name: zone.into(),
                        }
                    } else {
                        return Err(error("unsupported_policy"));
                    };
                    for role in roles {
                        if role.as_object().is_none_or(|v| {
                            v.keys()
                                .any(|k| !matches!(k.as_str(), "id" | "meta" | "name"))
                        }) {
                            return Err(error("unsupported_policy"));
                        }
                        let role_id = id(role)?;
                        let name = string(role, "name")?;
                        if role_names
                            .insert(role_id, name)
                            .is_some_and(|previous| previous != name)
                        {
                            return Err(error("unsupported_policy"));
                        }
                        let subject_id = match &subject {
                            Subject::Account(k) | Subject::Group(k) => &k.id,
                        };
                        let grant_id = format!(
                            "{subject_id}:policy:{policy_id}:role:{role_id}:{}",
                            resource.key.id
                        );
                        records.push((
                            Grant {
                                id: grant_id,
                                subject: subject.clone(),
                                resource: resource.key.clone(),
                                role: name.into(),
                                privilege: Privilege::Unknown,
                                certainty: Certainty::Observed,
                                provenance: provenance.clone(),
                            },
                            resource.clone(),
                        ));
                        if records.len() > MAX_ROWS {
                            return Err(error("limit"));
                        }
                    }
                }
            }
        }
        Ok(records)
    }
}
