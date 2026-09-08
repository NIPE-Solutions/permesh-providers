// SPDX-License-Identifier: MIT
//! Read-only Google Workspace Directory identity source.
use permesh_core::{Account, EntityKey, Identity, IdentityKind, IdentityStatus, Snapshot};
use permesh_provider_sdk::{
    Capability, Health, Metadata, Provider, ProviderError, ProviderFuture, validate_snapshot,
};
use permesh_secrets::Secret;
use reqwest::{Client, Url};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
mod client;
const MAX_PAGES: usize = 200;
const MAX_ROWS: usize = 100_000;
const PAGE_SIZE: usize = 500;
const VISIBILITY: &[&str] = &[
    "Google Directory users.list is an identity source for the configured customer. Collection is not transactional; directory visibility depends on the delegated administrator and OAuth scope.",
    "Only primaryEmail is directory-attested for exact email correlation; aliases, secondary and recovery emails are excluded. This is not proof of mailbox ownership or employee status.",
    "Identity kind is unknown. Active means both suspended and archived are explicitly false; absent status fields remain unknown. Deleted users, groups, memberships, resources and grants are not enumerated.",
    "OAuth access tokens are supplied directly or obtained once per invocation from the configured refresh credential; no tokens are persisted. Health checks probe at most one user and do not establish complete directory visibility.",
];
pub struct GoogleProvider {
    id: String,
    customer_id: String,
    token: Secret,
    client: Client,
    endpoint: Url,
    request_timeout: Duration,
}
pub fn provider_metadata() -> Metadata {
    Metadata {
        kind: "google".into(),
        capabilities: vec![Capability::Accounts, Capability::Identities],
    }
}
impl GoogleProvider {
    pub fn new(id: String, customer_id: String, token: Secret) -> Result<Self, ProviderError> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            || !(2..=128).contains(&customer_id.len())
            || !customer_id.starts_with('C')
            || !customer_id.bytes().all(|c| c.is_ascii_alphanumeric())
            || token.expose().is_empty()
            || id.contains(token.expose())
            || customer_id.contains(token.expose())
        {
            return Err(error("configuration"));
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("permesh/0.1")
            .build()
            .map_err(|_| error("configuration"))?;
        Ok(Self {
            id,
            customer_id,
            token,
            client,
            endpoint: Url::parse("https://admin.googleapis.com/admin/directory/v1/users")
                .map_err(|_| error("configuration"))?,
            request_timeout: Duration::from_secs(15),
        })
    }
    fn record(&self, value: &Value) -> Result<(Account, Identity, bool), ProviderError> {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| {
                !v.is_empty() && v.len() <= 128 && v.bytes().all(|c| c.is_ascii_alphanumeric())
            })
            .ok_or_else(|| error("malformed"))?;
        if value.get("customerId").and_then(Value::as_str) != Some(self.customer_id.as_str()) {
            return Err(error("customer"));
        }
        let email = value
            .get("primaryEmail")
            .and_then(Value::as_str)
            .filter(|v| valid_email(v))
            .ok_or_else(|| error("malformed"))?;
        // The host knows only the delivered refresh credential, not the newly
        // issued access token. Reject reflection before constructing any output
        // fields, including canonical IDs and directory-attested emails.
        let identity_id = format!("google:{}:{id}", self.customer_id);
        if id.contains(self.token.expose())
            || email.contains(self.token.expose())
            || identity_id.contains(self.token.expose())
        {
            return Err(error("malformed"));
        }
        let suspended = value.get("suspended").and_then(Value::as_bool);
        let archived = value.get("archived").and_then(Value::as_bool);
        let malformed = ["suspended", "archived"]
            .iter()
            .any(|key| value.get(key).is_some_and(|v| !v.is_boolean()));
        let status = if suspended == Some(true) || archived == Some(true) {
            IdentityStatus::Inactive
        } else if !malformed && suspended == Some(false) && archived == Some(false) {
            IdentityStatus::Active
        } else {
            IdentityStatus::Unknown
        };
        Ok((
            Account {
                key: EntityKey::new(&self.id, id),
                login: email.into(),
                kind: IdentityKind::Unknown,
                verified_emails: vec![email.into()],
            },
            Identity {
                id: identity_id,
                kind: IdentityKind::Unknown,
                status,
                verified_emails: vec![email.into()],
            },
            malformed,
        ))
    }
    async fn collect(&self) -> Result<Snapshot, ProviderError> {
        let mut snapshot = Snapshot::new(&self.id);
        snapshot.limitations = visibility();
        let mut records = BTreeMap::new();
        let mut ids = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        let mut token = None;
        let mut rows = 0usize;
        for page in 0..MAX_PAGES {
            let result = self.page(token.as_deref(), PAGE_SIZE).await;
            let values = match result {
                Ok(value) => value,
                Err(err) if page == 0 => return Err(err),
                Err(err) => {
                    fail(&mut snapshot, err);
                    break;
                }
            };
            let users = match users(&values) {
                Ok(users) => users,
                Err(err) if page == 0 => return Err(err),
                Err(err) => {
                    fail(&mut snapshot, err);
                    break;
                }
            };
            let remaining = MAX_ROWS.saturating_sub(rows);
            for value in users.iter().take(PAGE_SIZE.min(remaining)) {
                // A repeated native ID invalidates the earlier claim even if this record is malformed.
                let duplicate = value.get("id").and_then(Value::as_str).is_some_and(|id| {
                    if !ids.insert(id.to_owned()) {
                        records.remove(id);
                        true
                    } else {
                        false
                    }
                });
                if duplicate {
                    fail(&mut snapshot, error("duplicate"));
                    continue;
                }
                match self.record(value) {
                    Ok((account, identity, malformed)) => {
                        if malformed {
                            fail(&mut snapshot, error("malformed"));
                        }
                        records.insert(account.key.id.clone(), (account, identity));
                    }
                    Err(err) => fail(&mut snapshot, err),
                }
            }
            rows = rows.saturating_add(users.len());
            if users.len() > PAGE_SIZE || rows > MAX_ROWS {
                fail(&mut snapshot, error("limit"));
                break;
            }
            token = match values.get("nextPageToken") {
                None => break,
                Some(Value::String(value))
                    if !value.is_empty()
                        && value.len() <= 4096
                        && !value.chars().any(char::is_control)
                        && tokens.insert(value.clone()) =>
                {
                    Some(value.clone())
                }
                _ => {
                    fail(&mut snapshot, error("pagination"));
                    break;
                }
            };
            if page + 1 == MAX_PAGES || rows >= MAX_ROWS {
                fail(&mut snapshot, error("limit"));
                break;
            }
        }
        for (account, identity) in records.into_values() {
            snapshot.accounts.push(account);
            snapshot.identities.push(identity);
        }
        snapshot.sort();
        validate_snapshot(&snapshot).map_err(|_| error("malformed"))?;
        Ok(snapshot)
    }
}
impl Provider for GoogleProvider {
    fn metadata(&self) -> Metadata {
        provider_metadata()
    }
    fn check(&self) -> ProviderFuture<'_, Health> {
        Box::pin(async {
            let value = self.page(None, 1).await?;
            let users = users(&value)?;
            if users.len() > 1 {
                return Err(error("limit"));
            }
            for user in users {
                let (_, _, malformed) = self.record(user)?;
                if malformed {
                    return Err(error("malformed"));
                }
            }
            Ok(Health {
                message: "Google Directory users.list probe succeeded for the configured customer."
                    .into(),
                limitations: visibility(),
            })
        })
    }
    fn discover(&self) -> ProviderFuture<'_, Snapshot> {
        Box::pin(self.collect())
    }
}
fn users(value: &Value) -> Result<&[Value], ProviderError> {
    if !value.is_object() {
        return Err(error("malformed"));
    }
    match value.get("users") {
        None => Ok(&[]),
        Some(Value::Array(users)) => Ok(users),
        _ => Err(error("malformed")),
    }
}
fn valid_email(value: &str) -> bool {
    value.len() <= 320
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        })
}
fn visibility() -> Vec<String> {
    VISIBILITY.iter().map(|v| (*v).into()).collect()
}
fn fail(snapshot: &mut Snapshot, err: ProviderError) {
    snapshot.complete = false;
    if !snapshot.limitations.contains(&err.message) {
        snapshot.limitations.push(err.message);
    }
}
fn error(code: &str) -> ProviderError {
    ProviderError::new(
        code,
        match code {
            "configuration" => "Invalid Google Directory provider configuration or HTTP settings.",
            "unauthorized" => {
                "Google Directory rejected authentication; supply a valid OAuth access token."
            }
            "forbidden" => {
                "Google Directory denied access; require admin.directory.user.readonly OAuth scope and permission to read users across this customer."
            }
            "rate_limit" => {
                "Google Directory quota prevented collection within the retry budget; retry later."
            }
            "timeout" => "Google Directory request exceeded its deadline.",
            "transport" => "Google Directory request could not be completed.",
            "limit" => {
                "Google Directory collection reached a response, page or record safety limit."
            }
            "pagination" => "Google Directory returned invalid or repeated pagination metadata.",
            "customer" => {
                "Google Directory returned a user outside the configured customer or omitted customer identity; the record was excluded."
            }
            "duplicate" => {
                "Google Directory returned a duplicate user ID; all records with that ID were excluded."
            }
            "malformed" => "Google Directory returned an invalid response or record.",
            _ => "Google Directory returned an unsuccessful response.",
        },
    )
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

mod auth;

pub mod protocol;
