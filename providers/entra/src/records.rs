// SPDX-License-Identifier: MIT
//! Graph-specific object interpretation. Labels never become verified identity evidence.
use crate::error;
use permesh_core::{Account, Affiliation, EntityKey, Identity, IdentityKind, IdentityStatus};
use permesh_provider_sdk::ProviderError;
use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Object {
    pub id: String,
    pub display_name: Option<String>,
    pub user_principal_name: Option<String>,
    pub user_type: Option<String>,
    pub account_enabled: Option<bool>,
    pub service_principal_type: Option<String>,
    #[serde(rename = "@odata.type")]
    pub object_type: Option<String>,
}
impl Object {
    pub fn validate(&mut self) -> Result<(), ProviderError> {
        if !uuid(&self.id) {
            return Err(error("malformed"));
        }
        self.id.make_ascii_lowercase();
        for value in [
            &self.display_name,
            &self.user_principal_name,
            &self.user_type,
            &self.service_principal_type,
            &self.object_type,
        ]
        .into_iter()
        .flatten()
        {
            if value.len() > 4096 {
                return Err(error("limit"));
            }
        }
        Ok(())
    }
    pub fn named(&self) -> String {
        self.display_name
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| self.id.clone())
    }
    pub fn account(
        &self,
        instance: &str,
        tenant: &str,
        service: bool,
    ) -> Result<(Identity, Account), ProviderError> {
        let expected = if service {
            "#microsoft.graph.servicePrincipal"
        } else {
            "#microsoft.graph.user"
        };
        if self.object_type.as_ref().is_some_and(|s| s != expected) {
            return Err(error("malformed"));
        }
        let kind = if service
            && matches!(
                self.service_principal_type.as_deref(),
                Some("Application" | "ManagedIdentity" | "Legacy")
            ) {
            IdentityKind::Service
        } else {
            IdentityKind::Unknown
        };
        let affiliation = if !service && self.user_type.as_deref() == Some("Guest") {
            Affiliation::External
        } else {
            Affiliation::Unknown
        };
        let status = match self.account_enabled {
            Some(true) => IdentityStatus::Active,
            Some(false) => IdentityStatus::Inactive,
            None => IdentityStatus::Unknown,
        };
        let identity = Identity {
            id: format!("entra:{tenant}:{}", self.id),
            kind,
            affiliation,
            status,
            verified_emails: vec![],
        };
        let account = Account {
            key: object_key(instance, tenant, &self.id),
            login: self
                .user_principal_name
                .as_ref()
                .filter(|s| !s.is_empty() && !service)
                .cloned()
                .unwrap_or_else(|| self.named()),
            kind,
            affiliation,
            status,
            verified_emails: vec![],
        };
        Ok((identity, account))
    }
}
pub(crate) fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
pub(crate) fn object_key(instance: &str, tenant: &str, id: &str) -> EntityKey {
    EntityKey::new(instance, format!("object:{tenant}:{id}"))
}
pub(crate) fn group_key(instance: &str, tenant: &str, id: &str) -> EntityKey {
    EntityKey::new(instance, format!("group:{tenant}:{id}"))
}
