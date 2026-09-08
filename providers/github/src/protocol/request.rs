// SPDX-License-Identifier: MIT
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub organizations: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub token: Zeroizing<String>,
}
