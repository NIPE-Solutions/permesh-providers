# AWS IAM provider candidate

This native provider inventories policy attachments in one commercial AWS account.
It is a development candidate; synthetic tests are not live-account qualification.

## Authentication and configuration

Configuration contains `account_id` (twelve digits), optional `caller_role` (an exact
STS assumed-role name), and `region` (for
regional STS, for example `eu-west-1`). Credentials are named references:

| Credential | Reference example | Required |
|---|---|---|
| `access_key_id` | `env://AWS_ACCESS_KEY_ID` | Yes |
| `secret_access_key` | `env://AWS_SECRET_ACCESS_KEY` | Yes |
| `session_token` | `env://AWS_SESSION_TOKEN` | For temporary credentials |

Named `keychain://INSTANCE/FIELD` references are also supported by the host.
Use the interactive setup form to store references; never put credential values
in configuration, command arguments or source control. The host resolves the
references and passes credentials through the native protocol. Its cleared child
environment does not inherit an AWS SDK default credential chain.

The provider deliberately does not load profiles, shared files, SSO caches,
`credential_process`, container/instance metadata or ambient endpoint settings.
For an existing trusted AWS CLI profile, authenticate/refresh explicitly outside
Permesh, then use the CLI's `aws configure export-credentials --profile PROFILE
--format env` workflow to populate the host launch environment, or transfer the
three values into named keychain entries. The export contains secrets: keep it
out of captured logs and shell history. The CLI may execute providers configured
in that profile; review that profile before running it. Permesh never executes
that command. Native profile/SSO refresh remains unsupported. Temporary sessions
must be refreshed externally before another collection. Removing a local reference
does not revoke the upstream session or access key. See the official
[export-credentials reference](https://docs.aws.amazon.com/cli/latest/reference/configure/export-credentials.html).

The necessary inventory permission is:

```json
{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"iam:GetAccountAuthorizationDetails","Resource":"*"}]}
```

The adapter first calls STS `GetCallerIdentity` and requires the returned account
and ARN account to match configuration. AWS documents this identity call as
requiring no additional permission. See [STS identity](https://docs.aws.amazon.com/STS/latest/APIReference/API_GetCallerIdentity.html).

## Evidence and endpoints

All calls are signed POST requests using the official maintained AWS Rust SDK.
IAM uses `https://iam.amazonaws.com` with signing region `us-east-1`; STS uses
`https://sts.REGION.amazonaws.com`. Custom endpoints, proxies, redirects and
noncommercial partitions are unsupported. A check performs STS validation and
an IAM query with `MaxItems=1`; discovery requests 100 items per page with all five
entity filters. See [GetAccountAuthorizationDetails](https://docs.aws.amazon.com/IAM/latest/APIReference/API_GetAccountAuthorizationDetails.html).

| IAM observation | Graph representation |
|---|---|
| User / role | Account keyed by account ID and immutable UserId / RoleId; unknown principal kind |
| IAM group | Group keyed by account ID and immutable GroupId |
| User GroupList | Observed membership resolved to a unique returned group |
| Managed policy | Evidence resource keyed by owner scope and immutable PolicyId |
| Inline policy | Evidence resource keyed by principal's immutable ID and inline policy name |
| Policy attachment | Grant to its policy evidence resource, native policy name, observed certainty, unknown privilege |

IAM roles can be assumed by people or workloads; no role-assumption edges are
inferred. There are no verified emails or authoritative person identities. Inline
policy names are not immutable: delete/recreate can reuse the same resource key.
Managed-policy attachments lacking returned policy metadata are omitted, with a
partial warning, rather than assigned an invented immutable identity.

The grants establish attachments to policy evidence resources, including policies
containing denies. They are not allow decisions or access to the AWS resources
named inside policies. Policy documents, conditions, explicit denies, trust,
boundaries, SCPs/RCPs, session policies and resource policies are not evaluated or
emitted. Identity Center, organization enumeration, service resources and root
access are outside scope. The adapter cannot establish effective administrator
access. A completed collection is a nontransactional inventory, not proof of all
possible access.

## Bounds and failure behavior

The provider follows opaque markers only when `IsTruncated=true`, including short
pages. The generated SDK defaults a missing flag to false, so the transport also
requires one explicit boolean at the documented XML location. Missing/repeated
markers, contradictory terminal markers, conflicting IDs, cross-account ARNs and
unresolved references produce partial results. Source records resolve across all
successful pages. Failure before the first IAM page is an error; later service
failures preserve earlier observations with `complete=false`.

Limits: 100 IAM pages, 20,000 source rows, 20,000 graph records, 2 MiB per HTTP
response, 32 MiB of response bodies and 600 attempts per provider lifetime.
Connection timeout is 5 seconds; request/body and SDK attempt deadlines are 15
seconds; SDK operation deadline is 30 seconds; each check/discovery has a
50-second deadline. Standard SDK retries have at most three attempts per call.
Dropping the future cancels active requests and retry waits. Limit expansion and
overall timeout return errors rather than an unvalidated graph.

Raw errors, bodies, response headers, URLs and credential values are never
included in returned diagnostics. Names and IDs reflecting explicit credentials
are excluded. Protocol strings use the shared runtime's sanitized categories.
Credential wrappers and SDK credential storage clear their owned secret storage
on drop; transport/signing libraries can create temporary buffers, so complete
process-memory zeroization is not claimed.

## Development and qualification

Rust 1.94.1 is required by current SDK dependencies. This slice uses
`aws-sdk-iam 1.123.0`, `aws-sdk-sts 1.114.0` and `aws-credential-types 1.3.0`,
with SDK default HTTP features disabled. A bounded reqwest transport reuses the
workspace TLS stack; SigV4 signing and query serialization remain in the SDK.
The locked graph adds 32 packages over the three-provider base; no `aws-config`,
SSO client or process-provider chain is linked.

Run `cargo +1.94.1 test -p permesh-provider-aws --locked` and
`cargo +1.94.1 clippy -p permesh-provider-aws --all-targets --locked -- -D warnings`.
Tests cover signed endpoint scopes, native IDs, role semantics, policy evidence,
account mismatch, conflicts, unresolved references, pagination, partial denial,
credential reflection, redirects, response limits, bounded throttling, dropped
requests, native setup and queued cancellation. Live credential validation,
five-platform artifacts and release qualification remain publication gates.

Current 0.2.0 source candidates require [explicit negotiated-v1 adoption](negotiated-v1.md).

## Enterprise network settings

AWS does not implement the negotiated `network_v1` feature. It rejects that
handshake before credentials or API access; it cannot silently bypass an approved
proxy or CA requirement. See [current network support](network.md).

The optional `caller_role` binds STS to that exact role name before IAM reads; sessions
may change during an explicitly approved external refresh. Role paths are not inferred.
[Identity Center inventory](aws-identity-center.md) is a separate evaluation prerelease binary
with its own instance/store/account scope and network support.
