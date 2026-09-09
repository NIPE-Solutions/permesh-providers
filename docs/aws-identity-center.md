# AWS Identity Center provider candidate

`permesh-provider-aws-identity-center` is a separate native binary in the AWS crate,
source version 0.2.0. It is unpublished and has not been live-account qualified.
The existing [IAM attachment inventory](aws.md) remains a separate provider kind.
This candidate collects directory accounts, groups, direct memberships and
permission-set assignment evidence; it does not evaluate effective AWS permissions.

## Explicit scope and authentication

Configuration contains:

| Field | Meaning |
|---|---|
| `account_id` | Expected twelve-digit STS caller account |
| `region` | Explicit commercial AWS region containing the instance |
| `instance_arn` | Exact approved `arn:aws:sso:::instance/...` ARN |
| `identity_store_id` | Exact store associated with that instance |
| `accounts` | Nonempty, unique allowlist of at most 100 assignment account IDs |
| `caller_role` | Optional exact STS assumed-role name; required by host profile sources |
| `include_organizations` | Optional account-name enumeration, default false |

Supply all three named temporary credential references: `access_key_id`,
`secret_access_key` and `session_token`. Use approved `env://NAME` or
`keychain://INSTANCE/SLOT` references through the host. No profile, shared credentials
file, SSO cache, process credential provider, instance metadata or ambient credential
chain is opened by this binary. Credential acquisition/refresh belongs to the operator
or a separately supported explicit host source. Credentials must remain valid for the
operation; no browser or AWS CLI is spawned.

Every check/discovery first calls STS `GetCallerIdentity`. Account and ARN account
must match `account_id`. When `caller_role` is present, the ARN must have exact form
`arn:aws:sts::ACCOUNT:assumed-role/ROLE/SESSION`, with the approved role name and a
nonempty bounded session. IAM role paths are not inferred from that name.

Next, paginated `ListInstances` must contain the configured instance exactly once,
paired with the configured identity store. Failure stops directory enumeration.
A successful check proves these bindings, not all subsequent read permissions.
The caller may be an authorized delegated administrator; instance owner and caller
account need not be identical.

## Implemented reads and permission baseline

| Service action | Scope and retained evidence |
|---|---|
| STS `GetCallerIdentity` | Caller account/role proof only |
| `sso:ListInstances` | Selected instance/store binding only |
| `identitystore:ListUsers` | Selected store's directory accounts |
| `identitystore:ListGroups` | Selected store's groups |
| `identitystore:ListGroupMemberships` | Direct user memberships in observed groups |
| `sso:ListPermissionSetsProvisionedToAccount` | Permission sets for each allowlisted account |
| `sso:DescribePermissionSet` | Native name and exact ARN for each returned permission set |
| `sso:ListAccountAssignments` | User/group assignments for that account and permission set |
| `organizations:ListAccounts` | Only with the explicit flag; names for allowlisted accounts |

Review service-specific resource and condition scoping in the current
[AWS Service Authorization Reference](https://docs.aws.amazon.com/service-authorization/latest/reference/).
This table identifies implemented reads; it is not a universal IAM policy or claim
that every action supports identical resource restrictions. No write permission is
required. Delegation, organization membership and permission-set provisioning affect
which reads AWS permits.

Assignments are enumerated **only through permission sets reported provisioned to
an allowlisted account**. Unprovisioned or failed-provisioning assignments are not
independently enumerated. The latest permission-set document may differ from the
version provisioned to an account; policy contents and provisioning state are not
evaluated. See [provisioned permission sets](https://docs.aws.amazon.com/singlesignon/latest/APIReference/API_ListPermissionSetsProvisionedToAccount.html),
[permission-set description](https://docs.aws.amazon.com/singlesignon/latest/APIReference/API_DescribePermissionSet.html)
and [account assignments](https://docs.aws.amazon.com/singlesignon/latest/APIReference/API_ListAccountAssignments.html).

Organizations is off by default. Enabling it enumerates account metadata from the
organization API, but retains only allowlisted account names and never adds assignment
targets. It does not collect organization hierarchy, account lifecycle, email or SCPs.
Empty pages with a continuation token are followed, as required by the
[Organizations pagination contract](https://docs.aws.amazon.com/organizations/latest/APIReference/API_ListAccounts.html).
Configured account resources remain scope references when their reads fail; they do
not establish that an account exists or that access was granted.

## Record semantics

Account/group keys combine provider instance, selected region, identity-store ID and
native principal ID. Renamed labels do not change keys. Permission-set target keys
also include account ID and native permission-set ARN, with the account resource as
parent. Grant IDs retain account, permission-set ARN, principal type and native ID.
The role string preserves both the native name and ARN.

Directory users have unknown principal kind and affiliation. Explicit `UserStatus`
`ENABLED`/`DISABLED` becomes active/inactive; absent or future status remains unknown.
No human, employee or verified-email claim is made, and no canonical identity or
identity-authority capability is emitted. See the current
[Identity Store user schema](https://docs.aws.amazon.com/singlesignon/latest/IdentityStoreAPIReference/API_User.html).
Use explicit host mappings to associate these accounts with an independent authority.

Memberships remain [direct user-to-group observations](https://docs.aws.amazon.com/singlesignon/latest/IdentityStoreAPIReference/API_ListGroupMemberships.html).
Assignments retain observed assignment evidence and **unknown privilege**, including
permission sets named `AdministratorAccess`. No effective-access, role-assumption,
application-assignment, policy-condition, deny-precedence, session or employment
conclusion is inferred. Missing directory principals cause partial coverage and no
invented membership/grant. Conflicting IDs or scope fields fail closed.

## Transport, bounds and failures

Official AWS SDKs sign requests. Fixed HTTPS origins are regional STS, SSO and
Identity Store, plus `organizations.us-east-1.amazonaws.com`. Commercial regions only;
custom AWS endpoints, redirects and endpoint/profile environment overrides are not
accepted. Identity Center supports the existing `network_v1` context through the
shared explicit proxy/no-proxy/CA builder. The IAM binary's network support is unchanged.
Platform TLS verification remains enabled; process proxy environment is ignored.

Bounds are 50 seconds per provider operation, 20 seconds per SDK operation,
10 seconds per attempt, 5 seconds to connect, up to three SDK attempts,
1,000 total HTTP attempts, 2 MiB per response and 32 MiB total response bytes.
Collections allow at most 100 pages, request 100 rows per page (Organizations: 20),
and bound retained source/final graph rows to 20,000. Opaque continuation tokens are
limited to 4 KiB; longer otherwise-valid service tokens produce partial coverage.
No page-length heuristic establishes completion. Missing/null list arrays, duplicate
JSON keys and wrong token types are rejected before SDK defaults can imply an empty
complete result. Absent/null continuation means no further page; repeated/empty tokens
are rejected. Successful JSON that reflects a configured credential is rejected.

Validated prior pages survive later denied/throttled/transport failures as partial
observations. Malformed conflicting scope/record data or failed binding returns a
curated provider failure. Deadline and budget failures never establish absence of
access or account deactivation. Errors contain no raw AWS response bodies or secrets.

## Qualification and live acceptance

Local tests cover signed SDK HTTP requests, instance/store/account/role rejection,
allowlist isolation, direct group/user assignments, unknown privilege, disabled
lifecycle, missing list guards, credential reflection, later-page denial, opaque
pagination, optional organization naming and negotiated host decoding. Actual binary
tests cover setup/cancellation, invalid invocation and explicit CONNECT proxy routing
with hostile ambient proxy variables. The CONNECT fixture rejects locally; no AWS
endpoint is contacted. Shared runtime tests cover approved private-CA TLS behavior.

Run `cargo +1.94.1 test -p permesh-provider-aws` and
`cargo +1.94.1 clippy -p permesh-provider-aws --all-targets -- -D warnings`.
Build the explicit binary with
`cargo +1.94.1 build -p permesh-provider-aws --bin permesh-provider-aws-identity-center`.
Register its exact executable digest and capabilities, complete host trust/approval,
and select negotiated-v1 discovery before supplying credential references. There is
no catalog entry, released artifact or five-target qualification claim for this binary.

For a separately authorized live acceptance session:

1. Approve the exact binary, caller account/role, region, instance/store pair, account
   allowlist, read policy and network context. Start with a small test account scope.
2. Acquire short-lived credentials outside the adapter. Independently verify STS caller
   and instance/store metadata, then run check. Wrong account/role/store must fail.
3. Compare users, groups and direct memberships with independent Identity Store reads;
   include enabled/disabled users and missing-status observations where available.
4. Compare provisioned permission sets and direct/group assignments for each approved
   account. Preserve native IDs/ARNs; confirm administrator-like names stay unknown
   effective privilege. Record the provisioned-only boundary and any provisioning lag.
5. Exercise a paginated scope, restricted read and expired session. Confirm retained
   partial evidence or typed failure, never an inferred revocation or complete absence.
6. If explicitly approved, enable Organizations and verify only allowlisted names are
   retained. Confirm another organization account is never queried for assignments.
7. Record exact API date, source/binary digest, native platform, permission policy,
   supported scope and redacted results. Revoke test sessions through normal AWS
   administration. Until this is performed, live status remains **not run**.
