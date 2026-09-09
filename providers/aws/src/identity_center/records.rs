// SPDX-License-Identifier: MIT
use super::*;
use permesh_core::{
    Account, Affiliation, EntityKey, Group, IdentityKind, IdentityStatus, Resource,
};
pub(super) fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
pub(super) fn principal(s: &str) -> bool {
    uuid(s)
        || s.split_once('-').is_some_and(|(a, b)| {
            a.len() == 10 && a.bytes().all(|b| b.is_ascii_hexdigit()) && uuid(b)
        })
}
impl IdentityCenterProvider {
    pub(super) fn safe(&self, s: &str) -> bool {
        !s.is_empty()
            && s.len() <= 4096
            && !s.chars().any(char::is_control)
            && !crate::reflects(&self.credentials, s)
    }
    pub(super) fn key(&self, kind: &str, id: &str) -> EntityKey {
        EntityKey::new(
            &self.id,
            format!(
                "{}:{}:{kind}:{id}",
                self.config.region, self.config.identity_store_id
            ),
        )
    }
    pub(super) fn user(
        &self,
        v: &aws_sdk_identitystore::types::User,
    ) -> Result<Account, ProviderError> {
        if v.identity_store_id() != self.config.identity_store_id || !principal(v.user_id()) {
            return Err(error("malformed"));
        }
        let login = v.user_name().or(v.display_name()).unwrap_or(v.user_id());
        if !self.safe(login) {
            return Err(error("malformed"));
        }
        Ok(Account {
            key: self.key("user", v.user_id()),
            login: login.into(),
            kind: IdentityKind::Unknown,
            affiliation: Affiliation::Unknown,
            status: match v.user_status().map(|s| s.as_str()) {
                Some("ENABLED") => IdentityStatus::Active,
                Some("DISABLED") => IdentityStatus::Inactive,
                _ => IdentityStatus::Unknown,
            },
            verified_emails: vec![],
        })
    }
    pub(super) fn group(
        &self,
        v: &aws_sdk_identitystore::types::Group,
    ) -> Result<Group, ProviderError> {
        if v.identity_store_id() != self.config.identity_store_id || !principal(v.group_id()) {
            return Err(error("malformed"));
        }
        let name = v.display_name().unwrap_or(v.group_id());
        if !self.safe(name) {
            return Err(error("malformed"));
        }
        Ok(Group {
            key: self.key("group", v.group_id()),
            name: name.into(),
        })
    }
    pub(super) fn account_resource(&self, id: &str, name: &str) -> Resource {
        Resource {
            key: self.key("aws-account", id),
            name: name.into(),
            kind: Some("aws.account".into()),
            parent: None,
        }
    }
    pub(super) fn permission(&self, arn: &str) -> bool {
        let instance = self
            .config
            .instance_arn
            .strip_prefix("arn:aws:sso:::instance/")
            .unwrap_or_default();
        arn.strip_prefix(&format!("arn:aws:sso:::permissionSet/{instance}/ps-"))
            .is_some_and(|s| {
                s.len() == 16
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'/'))
            })
    }
}
