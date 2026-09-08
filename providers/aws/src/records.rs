// SPDX-License-Identifier: MIT
use super::*;
use aws_sdk_iam::{
    operation::get_account_authorization_details::GetAccountAuthorizationDetailsOutput,
    types::{
        AttachedPolicy, GroupDetail, ManagedPolicyDetail, PolicyDetail, RoleDetail, UserDetail,
    },
};
use permesh_core::{
    Account, Certainty, EntityKey, Grant, Group, IdentityKind, Membership, Privilege, Provenance,
    Resource, Subject,
};
use std::collections::BTreeMap;
#[derive(Default)]
pub(crate) struct Collected {
    users: Vec<UserDetail>,
    roles: Vec<RoleDetail>,
    groups: Vec<GroupDetail>,
    policies: Vec<ManagedPolicyDetail>,
    warnings: BTreeSet<String>,
}
impl Collected {
    pub fn partial(&mut self, code: &str) {
        self.warnings.insert(code.into());
    }
    pub fn add(&mut self, v: GetAccountAuthorizationDetailsOutput) -> Result<(), ProviderError> {
        self.users.extend(v.user_detail_list.unwrap_or_default());
        self.roles.extend(v.role_detail_list.unwrap_or_default());
        self.groups.extend(v.group_detail_list.unwrap_or_default());
        self.policies.extend(v.policies.unwrap_or_default());
        if self.users.len() + self.roles.len() + self.groups.len() + self.policies.len() > MAX_ROWS
        {
            return Err(error("limit"));
        }
        Ok(())
    }
    pub fn finish(mut self, p: &AwsProvider) -> Result<Snapshot, ProviderError> {
        let mut s = Snapshot::new(&p.id);
        s.limitations = VISIBILITY.iter().map(|v| (*v).into()).collect();
        let provenance = Provenance {
            method: "aws.iam.GetAccountAuthorizationDetails.attachment_inventory".into(),
            observed_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| error("malformed"))?,
        };
        let key = |v: String| EntityKey::new(&p.id, v);
        let policies = unique(
            std::mem::take(&mut self.policies),
            |v| v.policy_id(),
            &mut self.warnings,
        );
        let mut policy_map = BTreeMap::new();
        let mut policy_conflicts = BTreeSet::new();
        for (id, v) in policies {
            let (Some(name), Some(arn)) = (v.policy_name(), v.arn()) else {
                self.partial("malformed");
                continue;
            };
            if !safe(p, &id) || !native(&id, "ANPA") || !safe(p, name) || !policy_arn(p, name, arn)
            {
                self.partial("malformed");
                continue;
            }
            let resource = Resource {
                key: key(format!(
                    "policy:{}:{id}",
                    arn.split(':').nth(4).unwrap_or_default()
                )),
                name: name.into(),
            };
            if policy_map.insert(arn.to_owned(), resource).is_some() {
                policy_conflicts.insert(arn.to_owned());
                self.partial("conflicting_records")
            }
        }
        for arn in policy_conflicts {
            policy_map.remove(&arn);
        }
        s.resources.extend(policy_map.values().cloned());
        let groups = unique(
            std::mem::take(&mut self.groups),
            |v| v.group_id(),
            &mut self.warnings,
        );
        let mut group_map = BTreeMap::new();
        let mut group_conflicts = BTreeSet::new();
        let mut staged = Vec::new();
        for (id, v) in groups {
            let (Some(name), Some(arn)) = (v.group_name(), v.arn()) else {
                self.partial("malformed");
                continue;
            };
            if !safe(p, &id) || !native(&id, "AGPA") || !principal(p, name, arn, "group") {
                self.partial("malformed");
                continue;
            }
            let k = key(format!("group:{}:{id}", p.account));
            if group_map.insert(name.to_owned(), k.clone()).is_some() {
                group_conflicts.insert(name.to_owned());
                self.partial("conflicting_records")
            }
            staged.push((name.to_owned(), k, v));
        }
        for name in group_conflicts {
            group_map.remove(&name);
        }
        for (name, k, v) in staged {
            if group_map.get(&name) != Some(&k) {
                continue;
            }
            s.groups.push(Group {
                key: k.clone(),
                name,
            });
            self.attachments(
                p,
                &mut s,
                Subject::Group(k),
                v.attached_managed_policies(),
                v.group_policy_list(),
                &policy_map,
                &provenance,
            )?;
        }
        let users = unique(
            std::mem::take(&mut self.users),
            |v| v.user_id(),
            &mut self.warnings,
        );
        for (id, v) in users {
            let (Some(name), Some(arn)) = (v.user_name(), v.arn()) else {
                self.partial("malformed");
                continue;
            };
            if !safe(p, &id) || !native(&id, "AIDA") || !principal(p, name, arn, "user") {
                self.partial("malformed");
                continue;
            }
            let k = key(format!("user:{}:{id}", p.account));
            s.accounts.push(Account {
                key: k.clone(),
                login: name.into(),
                kind: IdentityKind::Unknown,
                verified_emails: vec![],
            });
            for group in v.group_list() {
                if let Some(g) = group_map.get(group) {
                    s.memberships.push(Membership {
                        member: Subject::Account(k.clone()),
                        group: g.clone(),
                        provenance: provenance.clone(),
                    })
                } else {
                    self.partial("unresolved_membership")
                }
            }
            self.attachments(
                p,
                &mut s,
                Subject::Account(k),
                v.attached_managed_policies(),
                v.user_policy_list(),
                &policy_map,
                &provenance,
            )?;
        }
        let roles = unique(
            std::mem::take(&mut self.roles),
            |v| v.role_id(),
            &mut self.warnings,
        );
        for (id, v) in roles {
            let (Some(name), Some(arn)) = (v.role_name(), v.arn()) else {
                self.partial("malformed");
                continue;
            };
            if !safe(p, &id) || !native(&id, "AROA") || !principal(p, name, arn, "role") {
                self.partial("malformed");
                continue;
            }
            let k = key(format!("role:{}:{id}", p.account));
            s.accounts.push(Account {
                key: k.clone(),
                login: name.into(),
                kind: IdentityKind::Unknown,
                verified_emails: vec![],
            });
            self.attachments(
                p,
                &mut s,
                Subject::Account(k),
                v.attached_managed_policies(),
                v.role_policy_list(),
                &policy_map,
                &provenance,
            )?;
        }
        if !self.warnings.is_empty() {
            s.complete = false;
            s.limitations.extend(
                self.warnings
                    .into_iter()
                    .map(|c| format!("AWS partial attachment collection: {c}.")),
            );
        }
        s.sort();
        s.memberships
            .dedup_by(|a, b| a.member == b.member && a.group == b.group);
        s.grants.dedup_by(|a, b| a.id == b.id);
        permesh_provider_sdk::validate_snapshot(&s)?;
        Ok(s)
    }
    #[allow(clippy::too_many_arguments)] // Explicit graph, evidence and provenance inputs.
    fn attachments(
        &mut self,
        p: &AwsProvider,
        s: &mut Snapshot,
        subject: Subject,
        attached: &[AttachedPolicy],
        inline: &[PolicyDetail],
        policies: &BTreeMap<String, Resource>,
        provenance: &Provenance,
    ) -> Result<(), ProviderError> {
        let sid = match &subject {
            Subject::Account(k) | Subject::Group(k) => k.id.clone(),
        };
        let mut targets = BTreeMap::new();
        for a in attached {
            let Some(resource) = a.policy_arn().and_then(|a| policies.get(a)) else {
                self.partial("unresolved_policy");
                continue;
            };
            if a.policy_name() != Some(resource.name.as_str()) {
                self.partial("conflicting_records");
                continue;
            }
            targets.insert(resource.key.clone(), resource.name.clone());
        }
        let mut names = BTreeSet::new();
        for a in inline {
            let Some(name) = a.policy_name().filter(|v| safe(p, v) && !v.contains(':')) else {
                self.partial("malformed");
                continue;
            };
            if !names.insert(name) {
                self.partial("conflicting_records");
                continue;
            }
            let resource = Resource {
                key: EntityKey::new(&p.id, format!("inline:{sid}:{name}")),
                name: name.into(),
            };
            targets.insert(resource.key.clone(), resource.name.clone());
            s.resources.push(resource);
        }
        for (resource, role) in targets {
            s.grants.push(Grant {
                id: format!("attachment:{sid}:{}", resource.id),
                subject: subject.clone(),
                resource,
                role,
                privilege: Privilege::Unknown,
                certainty: Certainty::Observed,
                provenance: provenance.clone(),
            });
        }
        if s.accounts.len()
            + s.groups.len()
            + s.resources.len()
            + s.memberships.len()
            + s.grants.len()
            > MAX_ROWS
        {
            return Err(error("limit"));
        }
        Ok(())
    }
}
fn unique<T: PartialEq>(
    rows: Vec<T>,
    id: impl Fn(&T) -> Option<&str>,
    warnings: &mut BTreeSet<String>,
) -> BTreeMap<String, T> {
    let mut map = BTreeMap::new();
    let mut blocked = BTreeSet::new();
    for row in rows {
        let Some(k) = id(&row).map(str::to_owned) else {
            warnings.insert("malformed".into());
            continue;
        };
        if blocked.contains(&k) {
            continue;
        }
        if map.get(&k).is_some_and(|old| old != &row) {
            map.remove(&k);
            blocked.insert(k);
            warnings.insert("conflicting_records".into());
        } else {
            map.insert(k, row);
        }
    }
    map
}
fn safe(p: &AwsProvider, s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 1024
        && !s.chars().any(char::is_control)
        && !reflects(&p.credentials, s)
}
fn native(s: &str, prefix: &str) -> bool {
    s.starts_with(prefix)
        && s.len() >= 16
        && s.len() <= 128
        && s.bytes().all(|b| b.is_ascii_alphanumeric())
}
fn principal(p: &AwsProvider, name: &str, arn: &str, kind: &str) -> bool {
    safe(p, name)
        && safe(p, arn)
        && arn.starts_with(&format!("arn:aws:iam::{}:{kind}/", p.account))
        && arn.rsplit('/').next() == Some(name)
}
fn policy_arn(p: &AwsProvider, name: &str, arn: &str) -> bool {
    safe(p, arn)
        && arn.rsplit('/').next() == Some(name)
        && (arn.starts_with(&format!("arn:aws:iam::{}:policy/", p.account))
            || arn.starts_with("arn:aws:iam::aws:policy/"))
}
