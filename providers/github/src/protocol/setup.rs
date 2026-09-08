// SPDX-License-Identifier: MIT OR Apache-2.0
use permesh_provider_sdk::setup::{Input, SetupField, SetupSpec, SetupStep};
pub(super) fn spec() -> SetupSpec {
    SetupSpec {
        schema_version:1,
        title:"GitHub organizations".into(),
        description:"Read organization membership and repository access from GitHub.com. Token permissions and organization authorization limit visibility.".into(),
        steps:vec![SetupStep{
            id:"connection".into(),title:"Connection".into(),description:"Use nonsecret organization names and a named credential reference.".into(),when:None,
            fields:vec![
                SetupField{key:"organizations".into(),label:"Organizations".into(),help:"Enter a JSON or YAML list of 1 to 100 GitHub organization names.".into(),required:true,default:None,when:None,input:Input::StringList{min_items:1,max_items:100}},
                SetupField{key:"token".into(),label:"Token reference".into(),help:"Use env://NAME or keychain://INSTANCE/token. Never enter the token itself here.".into(),required:true,default:None,when:None,input:Input::Credential},
            ],
        }],
    }
}
