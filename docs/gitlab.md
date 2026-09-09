# GitLab membership candidate

GitLab `0.2.0` is a published evaluation prerelease. It has not been qualified against a live GitLab.com tenant, self-managed version or
edition. Synthetic tests establish behavior against fixtures, not broad API or
platform compatibility. The candidate uses the existing native runtime and
negotiated v1 health/discovery, plus legacy draft 3 setup.

## Scope and authentication

Configure an explicit HTTPS **origin**, numeric `group_ids` and/or numeric
`project_ids`, and one `token` secret reference. Select at least one and at most
50 scopes in total. IDs are canonical positive decimal strings, not URL paths.
GitLab.com uses `https://gitlab.com`; a reviewed self-managed origin may use an
explicit port. Userinfo, HTTP, path prefixes, queries, fragments and redirects
are rejected. Installations hosted under a URL subpath are outside this slice.

Use a personal access token with `read_api`, owned by an account that can see the
intended resources. This scope is broader than metadata-only access; the adapter
performs only the GET requests below. Token permissions and resource membership
still limit visibility. `read_user` alone does not cover this inventory. Other
token types and self-managed edition/version combinations require separate
qualification. [GitLab token scopes](https://docs.gitlab.com/security/tokens/access_token_scopes/)
explain the upstream authority granted by each scope.

```yaml
# Provider-owned configuration, supplied through normal external setup:
origin: https://gitlab.com
group_ids: ["123"]
project_ids: ["456"]
# The credential slot is token; actual values stay outside configuration.
```

The host must approve this origin, scope, credential reference, executable pin
and any network context. The shared `network_v1` transport applies approved
proxy/bypass and additional CA settings to every HTTP request; it does not load
ambient proxies or change origins in response to API data. PAT refresh is
external; there is no OAuth, token-file or ambient credential fallback.

## Implemented observations

| Endpoint | Interpretation |
| --- | --- |
| `GET /api/v4/user` | Health authentication probe; not a statement of complete discovery permissions. |
| `GET /api/v4/groups/:id` and `/projects/:id` | Explicit resource metadata and stable numeric IDs. |
| `GET /api/v4/groups/:id/projects` | Directly owned visible projects, with `with_shared=false` and `include_subgroups=false`; returned namespace is checked. |
| `GET .../members` | Direct assignment evidence; direct group members also get an observed group-membership edge. |
| `GET .../members/all` | Observed effective membership, possibly collapsed to the highest role across inherited/shared paths; never labeled direct. |

Group resources and project resources retain observed containment only when the
parent group is also collected. Containment does not create grant inheritance.
No subgroup traversal, invited-group relationship graph, shared-project expansion
or fabricated group-to-project grants are emitted. Additional projects and
subgroups must be selected explicitly if they should be inspected.

Accounts use origin host/port plus immutable user ID within the configured
provider instance, for example `gitlab:gitlab.com:443:user:42`. Login changes do
not alter that key; the same numeric ID on another origin stays separate. Group
and project IDs have separate typed prefixes. API lifecycle is independent of
employment; absent/unknown states and principal kind remain unknown. An explicit
bot flag can identify a bot. No identity authority or verified email is asserted.

Native numeric access levels and returned `member_role_id` are retained in the
role descriptor. Maintainer/Admin and Owner receive their corresponding coarse
privilege categories; all other levels and custom roles remain unknown. The
adapter does not evaluate custom-role permissions, branch protections, inherited
path alternatives or complete effective authorization. Direct grants use
`assignment` evidence; `/members/all` uses `permission` evidence, with a
`members_all.effective_collapsed` provenance method. Both are API observations,
not proof that a particular action will succeed.

GitLab documents direct versus inherited membership and the highest-role
collapse in its [group member API](https://docs.gitlab.com/api/group_members/)
and [project member API](https://docs.gitlab.com/api/project_members/).
The [groups API](https://docs.gitlab.com/api/groups/) describes project filtering.
Permissions and response fields vary by API version, edition and requester.

## Bounds and limitations

Requests are reconstructed locally at 100 rows/page, at most 100 pages/list,
20,000 source rows and 20,000 final graph records, 2,000 request attempts and
32 MiB response bodies per operation. Each body is limited to 2 MiB; connections
to 5 seconds, requests to 15 seconds and operations to 50 seconds. Two retries
are allowed for 429/5xx, with numeric Retry-After no greater than 5 seconds.
Pagination headers/links must preserve origin, endpoint and filters without
skipping/repeating pages. Keyset pagination is not implemented.

Later-page/endpoint failures retain valid earlier observations and mark the
snapshot incomplete. Malformed/conflicting account observations suppress that
account's paths. Duplicate IDs invalidate the affected list. Overall bounds,
timeouts or an invalid final graph can reject the operation rather than return
unvalidated data. Permanent scope qualifiers remain even when collection finishes.
No visible configured resource is an error, not a complete empty access claim.

Expired membership rows are not treated as usable assignments; invalid/expired
expiry data makes the affected observation incomplete. Future expiry dates are
not exported as an access schedule. Invitations, tokens, sessions, provisioning,
ownership-transfer dependencies and revocation are not inventoried or performed.
Upstream errors and credential reflection produce curated failures without raw
payloads. Reports still contain sensitive access metadata.

## Build, install and qualify deliberately

Use the provider workspace's required Rust toolchain (currently 1.94.1) and an
exact reviewed revision:

```sh
cargo +1.94.1 test -p permesh-provider-gitlab --locked
cargo +1.94.1 clippy -p permesh-provider-gitlab --all-targets --locked -- -D warnings
cargo +1.94.1 build -p permesh-provider-gitlab --release --locked
```

Normal production native inspect/trust, setup and workspace approval remain
required. Download with `permesh provider install gitlab --version 0.2.0`
using CLI alpha.3, then register and trust the installed native executable. Use
`provider setup gitlab --id gitlab-main
--discovery-protocol negotiated-v1` only after explicitly registering the native
binary with its exact digest and accounts/resources/groups/memberships/grants
capabilities. Never approve a script or bypass trust to test a candidate.

Live acceptance is blocked until an authorized test tenant and PAT are available.
Record exact GitLab version/edition, CLI/provider revisions, origin, scope and
permissions privately. Then:

1. Confirm health and explicitly selected resources against independent GETs.
2. Compare one direct group member, direct project member, inherited member and
   a user with different direct/effective levels against both member endpoints.
3. Exercise an explicitly selected subgroup and a shared project; verify exclusions
   and parent containment match this documented scope.
4. Check pagination, denied scope, an expired/revoked test token, custom/unknown
   role handling and token-limited visibility. Incomplete evidence must stay partial.
5. For self-managed support, separately qualify the exact server version/edition
   and approved private CA/proxy route; redirects or foreign pagination must fail.
6. Confirm unchanged configuration and no credentials in retained stdout/stderr;
   retain only a sanitized acceptance record, not raw access exports.

These read-only checks do not authorize tenant changes, release publication or
catalog edits. See [qualification](qualification.md) and
[live acceptance](live-acceptance.md) for the wider release gates.
