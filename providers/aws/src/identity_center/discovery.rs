// SPDX-License-Identifier: MIT
use super::*;
use permesh_core::{
    Certainty, EvidenceKind, Grant, Membership, Privilege, Provenance, Resource, Subject,
};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Pager {
    pub token: Option<String>,
    seen: BTreeSet<String>,
    pages: usize,
}
impl Pager {
    pub fn advance(&mut self, token: Option<&str>) -> Result<bool, ProviderError> {
        self.pages += 1;
        match token {
            None => Ok(false),
            Some(v)
                if !v.is_empty()
                    && v.len() <= 4096
                    && !v.chars().any(char::is_control)
                    && self.seen.insert(v.into()) =>
            {
                if self.pages >= MAX_PAGES {
                    return Err(error("limit"));
                }
                self.token = Some(v.into());
                Ok(true)
            }
            _ => Err(error("pagination")),
        }
    }
}
fn partial(s: &mut Snapshot, code: &str) {
    s.complete = false;
    let code = match code {
        "forbidden" => "forbidden",
        "unauthorized" => "unauthorized",
        "rate_limit" => "rate_limit",
        "pagination" => "pagination",
        "limit" => "limit",
        "malformed" => "malformed",
        _ => "transport",
    };
    let message = format!("Identity Center partial collection: {code}.");
    if !s.limitations.contains(&message) {
        s.limitations.push(message);
    }
}
impl IdentityCenterProvider {
    pub(super) async fn collect(&self) -> Result<Snapshot, ProviderError> {
        self.prove().await?;
        let mut snapshot = Snapshot::new(&self.id);
        snapshot.limitations = VISIBILITY.iter().map(|s| (*s).into()).collect();
        let mut rows = 0usize;
        macro_rules! fetch {
            ($builder:expr,$field:ident) => {
                fetch!($builder, $field, 100)
            };
            ($builder:expr,$field:ident,$page_size:expr) => {{
                let mut pager = Pager::default();
                let mut out = Vec::new();
                loop {
                    let response = match ($builder)
                        .set_next_token(pager.token.clone())
                        .max_results($page_size)
                        .send()
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            partial(
                                &mut snapshot,
                                crate::service_code(e.as_service_error().and_then(|e| e.code())),
                            );
                            break;
                        }
                    };
                    rows = rows.saturating_add(response.$field().len());
                    if rows > MAX_ROWS {
                        partial(&mut snapshot, "limit");
                        break;
                    }
                    out.extend_from_slice(response.$field());
                    match pager.advance(response.next_token()) {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(e) => {
                            partial(&mut snapshot, &e.code);
                            break;
                        }
                    }
                }
                out
            }};
        }
        let users = fetch!(
            self.store
                .list_users()
                .identity_store_id(&self.config.identity_store_id),
            users
        );
        let mut user_keys = BTreeMap::new();
        for user in users {
            let account = self.user(&user)?;
            if user_keys
                .insert(user.user_id().to_owned(), account.key.clone())
                .is_some()
            {
                return Err(error("malformed"));
            }
            snapshot.accounts.push(account);
        }
        let groups = fetch!(
            self.store
                .list_groups()
                .identity_store_id(&self.config.identity_store_id),
            groups
        );
        let mut group_keys = BTreeMap::new();
        for group in groups {
            let record = self.group(&group)?;
            if user_keys.contains_key(group.group_id())
                || group_keys
                    .insert(group.group_id().to_owned(), record.key.clone())
                    .is_some()
            {
                return Err(error("malformed"));
            }
            snapshot.groups.push(record);
        }
        let provenance = Provenance {
            method: "aws.identity_center.direct_group_membership".into(),
            observed_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|_| error("malformed"))?,
        };
        let mut memberships = BTreeSet::new();
        let mut membership_ids = BTreeSet::new();
        for (id, key) in &group_keys {
            let members = fetch!(
                self.store
                    .list_group_memberships()
                    .identity_store_id(&self.config.identity_store_id)
                    .group_id(id),
                group_memberships
            );
            for member in members {
                if member.identity_store_id() != self.config.identity_store_id
                    || member.group_id() != Some(id.as_str())
                    || member.membership_id().is_none_or(|id| {
                        !records::principal(id) || !membership_ids.insert(id.to_owned())
                    })
                {
                    return Err(error("malformed"));
                }
                let user = match member.member_id() {
                    Some(aws_sdk_identitystore::types::MemberId::UserId(id)) => id,
                    _ => {
                        partial(&mut snapshot, "malformed");
                        continue;
                    }
                };
                let Some(user_key) = user_keys.get(user) else {
                    partial(&mut snapshot, "malformed");
                    continue;
                };
                if !memberships.insert((user_key.clone(), key.clone())) {
                    return Err(error("malformed"));
                }
                snapshot.memberships.push(Membership {
                    member: Subject::Account(user_key.clone()),
                    group: key.clone(),
                    provenance: provenance.clone(),
                });
            }
        }
        let mut account_names: BTreeMap<String, String> = self
            .config
            .accounts
            .iter()
            .map(|a| (a.clone(), a.clone()))
            .collect();
        if self.config.include_organizations {
            // The API enumerates the organization, but only configured accounts become resources.
            let accounts = fetch!(self.organizations.list_accounts(), accounts, 20);
            let mut seen = BTreeSet::new();
            for account in accounts {
                let Some(id) = account.id() else {
                    return Err(error("malformed"));
                };
                if !crate::valid_account(id) || !seen.insert(id.to_owned()) {
                    return Err(error("malformed"));
                }
                if let Some(name) = account_names.get_mut(id) {
                    let Some(value) = account.name().filter(|s| self.safe(s)) else {
                        return Err(error("malformed"));
                    };
                    *name = value.into();
                }
            }
            for id in &self.config.accounts {
                if !seen.contains(id) {
                    partial(&mut snapshot, "malformed");
                }
            }
        }
        let mut grants = BTreeSet::new();
        for (account, name) in account_names {
            let resource = self.account_resource(&account, &name);
            let resource_key = resource.key.clone();
            snapshot.resources.push(resource);
            let permissions = fetch!(
                self.sso
                    .list_permission_sets_provisioned_to_account()
                    .instance_arn(&self.config.instance_arn)
                    .account_id(&account),
                permission_sets
            );
            let mut seen = BTreeSet::new();
            for arn in permissions {
                if !self.permission(&arn) || !seen.insert(arn.clone()) {
                    return Err(error("malformed"));
                }
                let description = match self
                    .sso
                    .describe_permission_set()
                    .instance_arn(&self.config.instance_arn)
                    .permission_set_arn(&arn)
                    .send()
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        partial(
                            &mut snapshot,
                            crate::service_code(e.as_service_error().and_then(|e| e.code())),
                        );
                        continue;
                    }
                };
                let permission = description
                    .permission_set()
                    .ok_or_else(|| error("malformed"))?;
                let name = permission
                    .name()
                    .filter(|s| self.safe(s))
                    .ok_or_else(|| error("malformed"))?;
                if permission.permission_set_arn() != Some(arn.as_str()) {
                    return Err(error("malformed"));
                }
                let target = self.key("permission-set-assignment", &format!("{account}:{arn}"));
                snapshot.resources.push(Resource {
                    key: target.clone(),
                    name: name.into(),
                    kind: Some("aws.identity_center.permission_set".into()),
                    parent: Some(resource_key.clone()),
                });
                let assignments = fetch!(
                    self.sso
                        .list_account_assignments()
                        .instance_arn(&self.config.instance_arn)
                        .account_id(&account)
                        .permission_set_arn(&arn),
                    account_assignments
                );
                for assignment in assignments {
                    if assignment.account_id() != Some(account.as_str())
                        || assignment.permission_set_arn() != Some(arn.as_str())
                    {
                        return Err(error("malformed"));
                    }
                    let id = assignment
                        .principal_id()
                        .filter(|s| records::principal(s))
                        .ok_or_else(|| error("malformed"))?;
                    let subject = match assignment.principal_type().map(|s| s.as_str()) {
                        Some("USER") => user_keys.get(id).cloned().map(Subject::Account),
                        Some("GROUP") => group_keys.get(id).cloned().map(Subject::Group),
                        _ => None,
                    };
                    let Some(subject) = subject else {
                        partial(&mut snapshot, "malformed");
                        continue;
                    };
                    let grant_id = format!(
                        "{account}:{arn}:{}:{id}",
                        assignment
                            .principal_type()
                            .map(|s| s.as_str())
                            .unwrap_or_default()
                    );
                    if !grants.insert(grant_id.clone()) {
                        return Err(error("malformed"));
                    }
                    snapshot.grants.push(Grant {
                        id: grant_id,
                        subject,
                        resource: target.clone(),
                        role: format!("{name} ({arn})"),
                        privilege: Privilege::Unknown,
                        evidence_kind: EvidenceKind::Assignment,
                        certainty: Certainty::Observed,
                        provenance: Provenance {
                            method: "aws.identity_center.ListAccountAssignments".into(),
                            observed_at: provenance.observed_at.clone(),
                        },
                    });
                }
            }
        }
        if snapshot.accounts.len()
            + snapshot.groups.len()
            + snapshot.memberships.len()
            + snapshot.resources.len()
            + snapshot.grants.len()
            > MAX_ROWS
        {
            return Err(error("limit"));
        }
        snapshot.sort();
        snapshot.validate().map_err(|_| error("malformed"))?;
        Ok(snapshot)
    }
}
