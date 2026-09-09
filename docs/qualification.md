# Provider capability and qualification matrix

The historical four-provider audit covered revision
`5353ce802f158fa6ad5412f38b98721beb97cd5d` on 2026-09-09. The current-source rows
also describe GitLab, Entra and Identity Center. The exact seven-provider native
qualification record appears below; live acceptance remains separately scoped. These are implemented observations, not complete
effective authorization. Source candidates, native builds, live acceptance and catalog
availability are separate facts. The procedure for collecting new evidence is
[live acceptance](live-acceptance.md).

## Availability and host compatibility

| Artifact | Catalog/install status | Automated/native evidence | Live evidence |
| --- | --- | --- | --- |
| GitHub 0.1.0 | Published in [catalog](../catalog/v1.json), five targets; legacy discovery 2 and setup 3 | [Release record](releases/github-0.1.0.md): five native targets, package validation and subprocess tests | Documented Apple Silicon health/query/admin parity; limited to that recorded scope |
| GitHub 0.2.0 | Unpublished source candidate | [Seven-provider native evidence](#seven-provider-native-evidence) | [Limited local macOS ARM64 acceptance](#limited-local-github-acceptance); no reproducible release qualification |
| Google 0.2.0 | Unpublished source candidate; no Google catalog entry | [Seven-provider native evidence](#seven-provider-native-evidence) | No live acceptance record |
| Cloudflare 0.2.0 | Unpublished source candidate; no Cloudflare catalog entry | [Seven-provider native evidence](#seven-provider-native-evidence) | No live acceptance record |
| AWS IAM 0.2.0 | Unpublished source candidate; no AWS catalog entry | [Seven-provider native evidence](#seven-provider-native-evidence) | No live acceptance record |
| [GitLab 0.2.0](gitlab.md) | New unpublished source candidate; no catalog entry | Synthetic HTTP/runtime tests and [five-target candidate verification](#seven-provider-native-evidence) | No live GitLab.com or self-managed version/edition acceptance |
| [Entra 0.2.0](entra.md) | New unpublished source candidate; no catalog entry | Synthetic Graph/host/network tests and [five-target candidate verification](#seven-provider-native-evidence) | No live tenant/permission acceptance |
| [AWS Identity Center 0.2.0](aws-identity-center.md) | New unpublished source binary; no catalog entry | AWS SigV4 mock/host/network tests and [five-target candidate verification](#seven-provider-native-evidence) | No live account/permission acceptance |

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

## Seven-provider native evidence

[Native candidate run 34343615189](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34343615189)
passed on all five targets for PR source
[`ad5b348b6f37f2493d2c0729b715bafdc64e2877`](https://github.com/NIPE-Solutions/permesh-providers/commit/ad5b348b6f37f2493d2c0729b715bafdc64e2877).
The workflow tested merge commit
[`c0c86b7450091700959d7ec6971e6bbbd2eacfe0`](https://github.com/NIPE-Solutions/permesh-providers/commit/c0c86b7450091700959d7ec6971e6bbbd2eacfe0).
Both commits reference source tree
`bae6a7dd6086e60b3687b8155d7700e1978dd76a`.
[CI](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34343615210)
and [dependency checks](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34343615211)
also passed for that PR source.

The native jobs ran workspace and packaging tests, release builds, credential-free
setup and negotiated cancellation smoke checks, and archive verification for
GitHub, Google, Cloudflare, AWS IAM, GitLab, Entra and AWS Identity Center. The five
targets are macOS ARM64/Intel, Linux GNU ARM64/x86_64 and Windows MSVC x86_64.

All **35 candidate packages** from that same run were then downloaded and
independently checked with the existing package verifier. The check covered the
outer CI artifact digests, archive and executable SHA-256 values, checksum
sidecars, native target headers, strict file layout, and combined project/dependency
notice-bundle presence. Package identities matched their target and provider
folders. No downloaded executable was run during this independent verification.

These are unsigned trial artifacts with no release attestation. Hash and layout
verification does not authenticate a publisher, prove reproducible builds or
establish complete license compliance. The CI artifacts are temporarily retained;
no catalog entry or release was published by this qualification. This evidence
qualifies the exact source tree above, not the newer documentation commit or any
future source change. It does not establish live API permissions, API-path coverage,
other operating systems, or protocol stability. The limited local GitHub acceptance
below remains separate; there is no new live qualification for the other providers.

## Limited local GitHub acceptance

On 2026-09-09, a locally built GitHub candidate on macOS ARM64 passed explicitly
authorized read-only health, provider-status and stable-account JSON-query checks
through the current-source CLI, including explicit executable trust and workspace
approval. Results retained the expected visibility limitation and did not infer
verified email or canonical identity. Evidence binds the exact tested local
executable hashes; the shared build directory does not establish a reproducible
complete source revision or released artifact. This is limited GitHub acceptance,
not qualification of GitLab, Entra, Identity Center, other targets, every API path,
or publication readiness. Retained evidence records observed source-tree equivalence;
the exact full build commit was not proved.

## Implemented observations

A = accounts, I = identities, G = groups, M = memberships, R = resources,
E = grants/evidence. These names correspond to declared record capabilities.

| Provider and supported tenant/API | Records and stable keys | Authority and lifecycle | Access / ownership meaning |
| --- | --- | --- | --- |
| [GitHub](github.md): GitHub.com REST `2026-03-10`, configured organizations; no GHES/custom origin | A/G/M/R/E; account numeric IDs; organization/repository/team native IDs scoped to instance | No I or directory authority; API User/Bot kind, unknown affiliation/lifecycle; public emails omitted | Organization admin role means owner; repository native roles retained. Team relationship and collaborator effective-permission evidence can include inheritance; collaborator is not asserted direct. No ownership transfer/dependency inventory |
| [Google](google.md): Workspace Admin SDK Directory v1, one explicit stable `C…` customer; no `my_customer` or custom origin | A/I only; raw user IDs within instance, canonical `google:CUSTOMER:USER`; directory-attested primary email | Explicit host authority opt-in; archived → inactive, otherwise suspended → suspended, both false → active, missing flags → unknown; kind/affiliation unknown | No resource/grant/group/ownership collection; aliases, deleted users and recovery emails excluded |
| [Cloudflare](cloudflare.md): hosted account IAM and visible zones, one account ID; no custom origin/Zero Trust collection | A/G/M/R/E; account-member IDs, group IDs, zone IDs, account and policy-scope resources | No I or authority; kind/affiliation/lifecycle unknown; accepted membership is not proof of active identity | Exact/wildcard policy assignments with native roles and unknown effective privilege. Wildcards stay separate evidence resources; denies/unsupported scopes suppress affected paths. No inferred ownership or transfer dependencies |
| [AWS IAM](aws.md): one commercial AWS account, global IAM and configured regional STS; no GovCloud/China/Identity Center/Organizations | A/G/M/R/E; account-scoped immutable UserId/RoleId/GroupId/PolicyId; inline-policy key includes principal ID and mutable policy name | No I or authority; IAM users/roles retain unknown kind/affiliation/lifecycle | Observed policy attachments only, unknown privilege; no policy evaluation, role-assumption edges, root ownership or service resource inventory |

GitLab is a newer, separately tested source addition and is not covered by the historical four-provider run above. It emits A/G/M/R/E for explicitly scoped HTTPS-origin groups/projects: direct member assignments and separately labeled collapsed effective membership, with origin/native-ID keys, no email authority and no invented inheritance paths. See its [exact scope, permissions and live procedure](gitlab.md).

Entra is another separately tested source addition, outside the historical four-provider CI evidence. It emits A/I/G/M for one authenticated public Graph v1 tenant, using tenant/object UUIDs. Optional service-principal objects do not imply service-principal membership coverage. User kind and Member affiliation remain unknown; Guest is external directory affiliation, not employment. Explicit host authority and account mappings are required; UPN is not verified email. No grants, resources, directory roles, Azure RBAC, PIM, invitations, tokens or sessions are collected. See [Entra scope, sufficient read permissions, bounds and live acceptance](entra.md).

AWS Identity Center is a separate new binary; the earlier IAM evidence does not qualify it. It emits A/G/M/R/E for the approved instance/store and account allowlist, with provisioned-permission-set assignment evidence and unknown privilege. See [scope, permissions and live acceptance](aws-identity-center.md).

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
