// SPDX-License-Identifier: MIT OR Apache-2.0
//! GitHub organization graph traversal and lightweight authentication checks.
use crate::client::Budget;
use crate::records::{Collection, field, native_id, privilege, repository_role, safe_segment};
use crate::{GithubProvider, MAX_REQUESTS, MAX_ROWS, VISIBILITY, error};
use permesh_core::*;
use permesh_provider_sdk::{Capability, Health, Metadata, Provider, ProviderError, ProviderFuture};
use std::collections::{BTreeMap, BTreeSet};

impl GithubProvider {
    async fn collect(&self) -> Result<Snapshot, ProviderError> {
        let mut state = Collection::new(&self.id)?;
        let mut budget = Budget::default();
        for org in &self.organizations {
            if budget.requests >= MAX_REQUESTS || budget.rows >= MAX_ROWS {
                state.fail("discovery", error("limit"));
                break;
            }
            let organization = match self.object(&["orgs", org], &mut budget).await {
                Ok(v) => v,
                Err(e) => {
                    state.fail("organization", e);
                    continue;
                }
            };
            let Some(org_id) = native_id(&organization) else {
                state.fail("organization", error("malformed"));
                continue;
            };
            state.success = true;
            let org_key = state.key("organization", org_id);
            state.resources.insert(
                org_key.clone(),
                Resource {
                    key: org_key.clone(),
                    name: org.clone(),
                },
            );
            state.groups.insert(
                org_key.clone(),
                Group {
                    key: org_key.clone(),
                    name: org.clone(),
                },
            );
            // Separate member and owner filters preserve native organization roles.
            for (filter, role) in [("member", "member"), ("admin", "admin")] {
                let list = self
                    .list(&["orgs", org, "members"], &[("role", filter)], &mut budget)
                    .await;
                for user in state.accept("organization members", list) {
                    if let Some(account) = state.account(&user) {
                        state.membership(
                            Subject::Account(account.clone()),
                            org_key.clone(),
                            "github.organization_members",
                        );
                        state.grant(
                            Subject::Account(account),
                            org_key.clone(),
                            role,
                            if role == "admin" {
                                Privilege::Owner
                            } else {
                                Privilege::Standard
                            },
                            "github.organization_role",
                        );
                    }
                }
            }
            let list = self.list(&["orgs", org, "repos"], &[], &mut budget).await;
            let mut repositories = BTreeMap::new();
            for repo in state.accept("organization repositories", list) {
                if let Some((key, owner, name)) = state.repository(&repo) {
                    repositories.insert(key, (owner, name));
                }
            }
            let list = self.list(&["orgs", org, "teams"], &[], &mut budget).await;
            let mut seen_teams = BTreeSet::new();
            for team in state.accept("teams", list) {
                let (Some(id), Some(slug), Some(name)) =
                    (native_id(&team), field(&team, "slug"), field(&team, "name"))
                else {
                    state.fail("teams", error("malformed"));
                    continue;
                };
                if !safe_segment(slug) {
                    state.fail("teams", error("malformed"));
                    continue;
                }
                if !seen_teams.insert(id) {
                    continue;
                }
                let key = state.key("team", id);
                state.groups.insert(
                    key.clone(),
                    Group {
                        key: key.clone(),
                        name: format!("{org}/{name}"),
                    },
                );
                let list = self
                    .list(
                        &["orgs", org, "teams", slug, "members"],
                        &[("role", "all")],
                        &mut budget,
                    )
                    .await;
                for user in state.accept("team members", list) {
                    if let Some(account) = state.account(&user) {
                        state.membership(
                            Subject::Account(account),
                            key.clone(),
                            "github.team_members_effective",
                        );
                    }
                }
                let list = self
                    .list(&["orgs", org, "teams", slug, "repos"], &[], &mut budget)
                    .await;
                for repo in state.accept("team repositories", list) {
                    if let Some((resource, owner, name)) = state.repository(&repo) {
                        let permissions = match self
                            .url(&["orgs", org, "teams", slug, "repos", &owner, &name], &[])
                        {
                            Ok(url) => match self
                                .get_media(
                                    url,
                                    &mut budget,
                                    "application/vnd.github.v3.repository+json",
                                )
                                .await
                            {
                                Ok((_, value))
                                    if value.is_object()
                                        && native_id(&value) == native_id(&repo) =>
                                {
                                    Some(value)
                                }
                                Ok(_) => {
                                    state.fail("team repository permissions", error("malformed"));
                                    None
                                }
                                Err(e) => {
                                    state.fail("team repository permissions", e);
                                    None
                                }
                            },
                            Err(e) => {
                                state.fail("team repository permissions", e);
                                None
                            }
                        };
                        let role = permissions
                            .as_ref()
                            .map(repository_role)
                            .unwrap_or("unknown");
                        state.grant(
                            Subject::Group(key.clone()),
                            resource.clone(),
                            role,
                            privilege(role),
                            "github.team_repository_effective",
                        );
                        repositories.insert(resource, (owner, name));
                    }
                }
            }
            for (resource, (owner, name)) in repositories {
                if budget.requests >= MAX_REQUESTS || budget.rows >= MAX_ROWS {
                    state.fail("collaborators", error("limit"));
                    break;
                }
                let list = self
                    .list(
                        &["repos", &owner, &name, "collaborators"],
                        &[("affiliation", "all")],
                        &mut budget,
                    )
                    .await;
                for user in state.accept("collaborators", list) {
                    if let Some(account) = state.account(&user) {
                        let role = repository_role(&user);
                        state.grant(
                            Subject::Account(account),
                            resource.clone(),
                            role,
                            privilege(role),
                            "github.repository_collaborator_effective",
                        );
                    }
                }
            }
        }
        state.finish()
    }
}

/// Static capabilities; does not construct a provider or resolve credentials.
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "github".into(),
        capabilities: vec![
            Capability::Accounts,
            Capability::Resources,
            Capability::Groups,
            Capability::Memberships,
            Capability::Grants,
        ],
    }
}

impl Provider for GithubProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async move {
            let mut budget = Budget::default();
            let user = self.object(&["user"], &mut budget).await?;
            if native_id(&user).is_none() || field(&user, "login").is_none() {
                return Err(error("malformed"));
            }
            for org in &self.organizations {
                let membership = self
                    .object(&["user", "memberships", "orgs", org], &mut budget)
                    .await?;
                if field(&membership, "state") != Some("active") {
                    return Err(error("membership"));
                }
            }
            Ok(Health {
                message: "GitHub authentication and active organization membership verified."
                    .into(),
                limitations: VISIBILITY.iter().map(|v| (*v).into()).collect(),
            })
        })
    }
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        Box::pin(self.collect())
    }
}
