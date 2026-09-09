// SPDX-License-Identifier: MIT
//! Directory enumeration retains validated earlier pages when later scopes fail.
use crate::{
    EntraProvider,
    client::{Budget, Listing},
    error,
    records::{group_key, object_key},
};
use permesh_core::{Group, Membership, Provenance, Snapshot, Subject};
use permesh_provider_sdk::ProviderError;
use std::collections::{BTreeMap, BTreeSet};
fn partial(snapshot: &mut Snapshot, code: &str) {
    snapshot.complete = false;
    let message = match code {
        "unauthorized" => "Entra partial collection: unauthorized.",
        "forbidden" => "Entra partial collection: forbidden.",
        "rate_limit" => "Entra partial collection: rate_limit.",
        "limit" => "Entra partial collection: limit.",
        "pagination" => "Entra partial collection: pagination.",
        "malformed" => "Entra partial collection: malformed.",
        "unsupported_member" => "Entra partial collection: unsupported member type.",
        "missing_member" => {
            "Entra partial collection: referenced member was not observed in its directory listing."
        }
        "timeout" => "Entra partial collection: timeout.",
        _ => "Entra partial collection: transport.",
    };
    if !snapshot.limitations.iter().any(|s| s == message) {
        snapshot.limitations.push(message.into());
    }
}
fn accept(snapshot: &mut Snapshot, list: Listing) -> Vec<crate::records::Object> {
    if let Some(code) = list.failure {
        partial(snapshot, code);
    }
    list.rows
}
impl EntraProvider {
    pub(crate) async fn collect(&self) -> Result<Snapshot, ProviderError> {
        let mut budget = Budget::default();
        self.prove_tenant(&mut budget).await?;
        let mut snapshot = Snapshot::new(&self.id);
        snapshot.limitations = self.limitations();
        let mut accounts = BTreeMap::new();
        let mut groups = BTreeMap::new();
        for (service, path, select) in [
            (
                false,
                "users",
                "id,displayName,userPrincipalName,accountEnabled,userType",
            ),
            (
                true,
                "servicePrincipals",
                "id,displayName,accountEnabled,servicePrincipalType",
            ),
        ] {
            if service && !self.include_service_principals {
                continue;
            }
            let list = self
                .listing(self.url(path, Some(select))?, &mut budget)
                .await;
            for object in accept(&mut snapshot, list) {
                match object.account(&self.id, &self.tenant_id, service) {
                    Ok((identity, account)) => {
                        if accounts.contains_key(&object.id) {
                            partial(&mut snapshot, "malformed");
                            continue;
                        }
                        accounts.insert(object.id, service);
                        snapshot.identities.push(identity);
                        snapshot.accounts.push(account);
                    }
                    Err(_) => partial(&mut snapshot, "malformed"),
                }
            }
        }
        let list = self
            .listing(self.url("groups", Some("id,displayName"))?, &mut budget)
            .await;
        for object in accept(&mut snapshot, list) {
            if object
                .object_type
                .as_ref()
                .is_some_and(|s| s != "#microsoft.graph.group")
                || accounts.contains_key(&object.id)
            {
                partial(&mut snapshot, "malformed");
                continue;
            }
            let group = Group {
                key: group_key(&self.id, &self.tenant_id, &object.id),
                name: object.named(),
            };
            groups.insert(object.id, group.clone());
            snapshot.groups.push(group);
        }
        let observed_at = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|_| error("malformed"))?;
        let mut memberships = BTreeSet::new();
        for (id, group) in &groups {
            let list = self
                .listing(
                    self.url(&format!("groups/{id}/members"), None)?,
                    &mut budget,
                )
                .await;
            for member in accept(&mut snapshot, list) {
                let subject = match member.object_type.as_deref() {
                    Some("#microsoft.graph.user") if accounts.get(&member.id) == Some(&false) => {
                        Subject::Account(object_key(&self.id, &self.tenant_id, &member.id))
                    }
                    Some("#microsoft.graph.group") if groups.contains_key(&member.id) => {
                        Subject::Group(group_key(&self.id, &self.tenant_id, &member.id))
                    }
                    // The stable public v1 API omits these unpredictably: exclude the entire unsupported scope.
                    Some("#microsoft.graph.servicePrincipal") => continue,
                    Some("#microsoft.graph.user" | "#microsoft.graph.group") => {
                        partial(&mut snapshot, "missing_member");
                        continue;
                    }
                    _ => {
                        partial(&mut snapshot, "unsupported_member");
                        continue;
                    }
                };
                if memberships.insert((subject.clone(), group.key.clone())) {
                    snapshot.memberships.push(Membership {
                        member: subject,
                        group: group.key.clone(),
                        provenance: Provenance {
                            method: "entra.direct_group_membership".into(),
                            observed_at: observed_at.clone(),
                        },
                    });
                }
            }
        }
        snapshot.sort();
        snapshot.validate().map_err(|_| error("malformed"))?;
        Ok(snapshot)
    }
}
