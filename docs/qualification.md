# Provider capability and qualification matrix

Audited against provider revision `5353ce802f158fa6ad5412f38b98721beb97cd5d`
on 2026-09-09. This describes implemented observations, not complete effective
authorization. Source candidates, native builds, live acceptance and catalog
availability are separate facts. The procedure for collecting new evidence is
[live acceptance](live-acceptance.md).

## Availability and host compatibility

| Artifact | Catalog/install status | Automated/native evidence | Live evidence |
| --- | --- | --- | --- |
| GitHub 0.1.0 | Published in [catalog](../catalog/v1.json), five targets; legacy discovery 2 and setup 3 | [Release record](releases/github-0.1.0.md): five native targets, package validation and subprocess tests | Documented Apple Silicon health/query/admin parity; limited to that recorded scope |
| GitHub 0.2.0 | Unpublished source candidate | Shared candidate evidence below | No 0.2.0 live qualification record; 0.1.0 evidence does not qualify new bytes |
| Google 0.2.0 | Unpublished source candidate; no Google catalog entry | Shared candidate evidence below | No live acceptance record |
| Cloudflare 0.2.0 | Unpublished source candidate; no Cloudflare catalog entry | Shared candidate evidence below | No live acceptance record |
| AWS IAM 0.2.0 | Unpublished source candidate; no AWS catalog entry | Shared candidate evidence below | No live acceptance record |

The audited candidate dependency pin is Permesh
`0196197a215c0b244251fe1e3ca1eb759159776b`. Use that compatible host revision or a
qualified successor; a minimum published CLI version is not established by this
source pin. Candidates use negotiated `protocol_version: 1` for health/discovery
and legacy draft 3 for setup; Google also declares draft 4 browser authentication.
The catalog selector is `discovery_protocol: negotiated_v1`, with `protocols: [3]`
for legacy setup. Do not change the published GitHub 0.1.0 selector to match a
candidate. The negotiated protocol remains a draft.

[Candidate run 34300167607](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34300167607)
succeeded for source `40cc241367b28e4b67113d6d03afeefd50a7f9b3`, the reviewed PR
head merged by the audited revision. Its five native jobs run workspace tests,
packaging tests, release builds, credential-free setup/handshake smoke checks and
archive verification for all four providers. Targets are macOS ARM64/Intel,
Linux GNU ARM64/x86_64 and Windows MSVC x86_64. Candidate artifacts are temporary,
unsigned build outputs; successful jobs do not make them installable releases.
[Network implementation CI](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34297605972)
passed Linux/macOS/Windows and Rust 1.94.1 checks. These references qualify their
exact source revisions, not future changes or older operating systems.

## Implemented observations

A = accounts, I = identities, G = groups, M = memberships, R = resources,
E = grants/evidence. These names correspond to declared record capabilities.

| Provider and supported tenant/API | Records and stable keys | Authority and lifecycle | Access / ownership meaning |
| --- | --- | --- | --- |
| [GitHub](github.md): GitHub.com REST `2026-03-10`, configured organizations; no GHES/custom origin | A/G/M/R/E; account numeric IDs; organization/repository/team native IDs scoped to instance | No I or directory authority; API User/Bot kind, unknown affiliation/lifecycle; public emails omitted | Organization admin role means owner; repository native roles retained. Team relationship and collaborator effective-permission evidence can include inheritance; collaborator is not asserted direct. No ownership transfer/dependency inventory |
| [Google](google.md): Workspace Admin SDK Directory v1, one explicit stable `C…` customer; no `my_customer` or custom origin | A/I only; raw user IDs within instance, canonical `google:CUSTOMER:USER`; directory-attested primary email | Explicit host authority opt-in; archived → inactive, otherwise suspended → suspended, both false → active, missing flags → unknown; kind/affiliation unknown | No resource/grant/group/ownership collection; aliases, deleted users and recovery emails excluded |
| [Cloudflare](cloudflare.md): hosted account IAM and visible zones, one account ID; no custom origin/Zero Trust collection | A/G/M/R/E; account-member IDs, group IDs, zone IDs, account and policy-scope resources | No I or authority; kind/affiliation/lifecycle unknown; accepted membership is not proof of active identity | Exact/wildcard policy assignments with native roles and unknown effective privilege. Wildcards stay separate evidence resources; denies/unsupported scopes suppress affected paths. No inferred ownership or transfer dependencies |
| [AWS IAM](aws.md): one commercial AWS account, global IAM and configured regional STS; no GovCloud/China/Identity Center/Organizations | A/G/M/R/E; account-scoped immutable UserId/RoleId/GroupId/PolicyId; inline-policy key includes principal ID and mutable policy name | No I or authority; IAM users/roles retain unknown kind/affiliation/lifecycle | Observed policy attachments only, unknown privilege; no policy evaluation, role-assumption edges, root ownership or service resource inventory |

## Authentication and visibility

| Provider | Implemented credentials and minimum documented scope | Transport / known restrictions |
| --- | --- | --- |
| GitHub | PAT or App user token in `token`; health requires `/user` and active org membership, excluding installation tokens. Classic PAT `read:org` + `repo` for full supported scope; fine-grained Members read + Metadata read and selected repositories. Collaborator listing additionally needs authenticated-user write/maintain/admin access | Classic `repo` is broader than read-only; GET-only adapter behavior does not reduce token authority. SSO/approval/selected repos/secret teams restrict visibility. No enterprise policy, base-permission or explicit nested-team inventory |
| Google | Admin SDK enabled; delegated administrator permitted to read customer users; `admin.directory.user.readonly`. Access `token`, or `refresh_token` + `client_secret` with nonsecret `client_id`; one refresh exchange per invocation. Host browser flow uses the provider declaration for refresh mode | No service-account JWT/domain-wide delegation or ADC implementation. Explicit network policy supported for API and refresh; host browser login with explicit policy is unsupported. Issued tokens are not persisted by provider |
| Cloudflare | Account-owned or user API `token`; Account Settings Read plus Zone Read for intended account/zones | Health probes account and bounded members, not all group/zone permissions. No token creation/revocation. Account-owned token creation itself requires appropriate upstream administration |
| AWS IAM | Explicit `access_key_id`, `secret_access_key`, optional temporary `session_token`; `iam:GetAccountAuthorizationDetails` on `*`; STS GetCallerIdentity account check | No ambient credential chain, profile/SSO refresh, credential process, custom endpoint or explicit network feature. Session refresh happens externally. IAM inventory permission is account-wide |

GitHub/Google/Cloudflare opt in to approved `network_v1` HTTPS proxy/bypass and
additional CA roots; AWS rejects it before credentials. Redirects are disabled.
All four omit raw upstream diagnostic payloads. See provider pages for endpoint
permission references and exact normalization rules.

| Provider | Tokens / sessions inventoried? | Pending invitations? | Bounds and partial behavior |
| --- | --- | --- | --- |
| GitHub | No token/key/session inventory or revocation | No | 100 rows/page, 100 pages/list, 20,000 source rows, 2,000 attempts; 2 MiB/body; 15 s/request, at most two retries. Rate-delay >60 s fails. Useful org observations survive endpoint failures; no visible org is an error |
| Google | No inventory/revocation; OAuth exchange is credential use only | No | 500 rows/page, 200 pages, 100,000 source rows; 2 MiB/body, 15 s/request, two GET retries; retry delay >5 s fails. Refresh: 64 KiB/body, 16 KiB token, no retries. Later-page errors preserve earlier users; duplicate IDs invalidate conflicts |
| Cloudflare | No API-token/session inventory or revocation | Pending members are recognized and excluded from accepted access; no invitation record capability | 50 rows/page, 100 pages/list, 20,000 rows and assignment-expansion bound, 2,000 attempts, 2 MiB/body; 15 s/request, two retries, Retry-After ≤30 s. Missing account is error; later list failures preserve useful data |
| AWS IAM | No access-key/session inventory or revocation | No | 100 pages, 100 requested items/page, 20,000 rows/graph records; 2 MiB/body, 32 MiB total bodies, 600 attempts/provider lifetime; three SDK attempts/call, 30 s SDK operation, 50 s check/discovery. First IAM-page failure is error; later failure preserves earlier records |

Shared runtime bounds are 55 seconds/operation, 1 MiB/frame, 64 MiB/transcript
and 100,000 output records. Google emits two records per user, so more than
50,000 paired users cannot be returned through this contract even though the API
collector's source-row bound is larger. Budget/time failures can return an error
rather than an unvalidated graph. Permanent visibility limitations still apply
when `complete=true`; completion means bounded collection finished, not universal
visibility or absence of access.

The host's synthetic multi-provider workflow test composes directory-shaped and
GitHub-shaped wire records plus a separately labeled service-account fixture. It
checks reviewed stable mappings, renamed labels, inactive identities, bot/service
separation, inherited paths, partial authority and failure isolation. It neither
executes these API adapters nor qualifies their OAuth permissions or real tenants.
