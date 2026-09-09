// SPDX-License-Identifier: MIT
use crate::{
    client::Budget,
    records::{self, Collection},
    *,
};
use permesh_core::{EntityKey, Group};
use serde_json::Value;
use std::collections::BTreeMap;
impl GitlabProvider {
    pub(crate) async fn collect(&self) -> Result<Snapshot, ProviderError> {
        let mut collection = Collection::new(&self.id, &self.scope)?;
        let mut budget = Budget::default();
        let mut groups = BTreeMap::new();
        let mut projects = BTreeSet::new();
        let mut parents = BTreeMap::new();
        for id in &self.group_ids {
            let loaded = async {
                let value = self
                    .request(&format!("groups/{id}"), &[], &mut budget)
                    .await?
                    .0;
                let resource = records::resource(&self.id, &self.scope, "group", *id, &value)?;
                let parent = records::parent(&value, "group")?;
                Ok::<_, ProviderError>((resource, parent))
            }
            .await;
            match loaded {
                Ok((resource, parent)) => {
                    parents.insert(resource.key.clone(), parent);
                    groups.insert(*id, resource.key.clone());
                    collection.groups.insert(
                        resource.key.clone(),
                        Group {
                            key: resource.key.clone(),
                            name: resource.name.clone(),
                        },
                    );
                    collection.resources.insert(resource.key.clone(), resource);
                }
                Err(failure) => collection.fail(failure),
            }
        }
        for id in groups.keys() {
            let listing = self
                .listing(
                    &format!("groups/{id}/projects"),
                    &[
                        ("with_shared", "false".into()),
                        ("include_subgroups", "false".into()),
                    ],
                    &mut budget,
                )
                .await;
            if let Some(failure) = listing.failure {
                collection.fail(failure);
            }
            for value in listing.rows {
                let parsed = records::native_id(&value).and_then(|project| {
                    if records::parent(&value, "project")? != Some(*id) {
                        return Err(error("scope"));
                    }
                    Ok(project)
                });
                match parsed {
                    Ok(project) => {
                        projects.insert(project);
                    }
                    Err(failure) => collection.fail(failure),
                }
            }
        }
        projects.extend(self.project_ids.iter().copied());
        if projects.len() > MAX_ROWS {
            return Err(error("limit"));
        }
        let mut observed_projects = BTreeMap::new();
        for id in projects {
            let loaded = async {
                let value = self
                    .request(&format!("projects/{id}"), &[], &mut budget)
                    .await?
                    .0;
                let resource = records::resource(&self.id, &self.scope, "project", id, &value)?;
                let parent = records::parent(&value, "project")?;
                Ok::<_, ProviderError>((resource, parent))
            }
            .await;
            match loaded {
                Ok((mut resource, parent)) => {
                    // Recheck scope after discovery; a moved project must be
                    // explicitly selected or still belong to an observed group.
                    if !self.project_ids.contains(&id)
                        && !parent.is_some_and(|id| groups.contains_key(&id))
                    {
                        collection.fail(error("scope"));
                        continue;
                    }
                    resource.parent = parent.and_then(|id| groups.get(&id).cloned());
                    observed_projects.insert(id, resource.key.clone());
                    collection.resources.insert(resource.key.clone(), resource);
                }
                Err(failure) => collection.fail(failure),
            }
        }
        for (key, parent) in parents {
            if let Some(parent) = parent.and_then(|id| groups.get(&id).cloned()) {
                if parent == key {
                    collection.fail(error("malformed"));
                } else if let Some(resource) = collection.resources.get_mut(&key) {
                    resource.parent = Some(parent);
                }
            }
        }
        if collection.resources.is_empty() {
            return Err(error("forbidden"));
        }
        for (kind, scopes) in [("groups", groups), ("projects", observed_projects)] {
            for (id, key) in scopes {
                for effective in [false, true] {
                    let path =
                        format!("{kind}/{id}/members{}", if effective { "/all" } else { "" });
                    let listing = self.listing(&path, &[], &mut budget).await;
                    if let Some(failure) = listing.failure {
                        collection.fail(failure);
                    }
                    self.members(
                        &mut collection,
                        &key,
                        kind == "groups",
                        effective,
                        listing.rows,
                    );
                }
            }
        }
        collection.finish()
    }
    fn members(
        &self,
        collection: &mut Collection,
        resource: &EntityKey,
        group: bool,
        effective: bool,
        rows: Vec<Value>,
    ) {
        for value in rows {
            if let Err(failure) = collection.member(resource, group, effective, &value) {
                collection.fail(failure);
            }
        }
    }
}
