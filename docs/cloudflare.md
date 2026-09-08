# Cloudflare provider candidate

This read-only native adapter collects one configured Cloudflare account's
members, IAM user groups, group memberships, policy assignments and visible
zones. It is a development candidate: synthetic and native protocol tests do not
constitute live-account validation or a supported published release.

## Configuration and credentials

The external provider accepts exactly one `account_id` setting (32 lowercase
hexadecimal characters) and a named `token` credential. Use a secret reference
such as `env://CLOUDFLARE_API_TOKEN` or a named keychain entry through Permesh.
Never put credentials in configuration or command-line arguments. The setup form
requests the account ID and credential reference; it does not create a token.

Use an account-owned API token for durable automation, or a user API token, with:

- Account / Account Settings / Read on the configured account.
- Zone / Zone / Read on the zones to include in that account.

Account-owned tokens support account and zone management. Creating or updating
one requires a Super Administrator; the adapter performs neither operation.
Revoke the token in Cloudflare's account/user API token dashboard. Removing the
local keychain entry does not revoke upstream credentials. See [account API
tokens](https://developers.cloudflare.com/fundamentals/api/get-started/account-owned-tokens/).

Only GET requests go to `https://api.cloudflare.com/client/v4/`. Redirects and
custom endpoints are disabled. Health checks account details and at most five
member records. A successful health check does not prove group/zone visibility
or complete discovery permission. It supports both token types without requiring
the user-token-specific verification endpoint.

## Collection and meaning

| Observation | GET endpoint |
|---|---|
| Account resource | `/accounts/{account_id}` |
| Account-member principals and policies | `/accounts/{account_id}/members` |
| IAM user groups and policies | `/accounts/{account_id}/iam/user_groups` |
| IAM group membership | `/accounts/{account_id}/iam/user_groups/{group_id}/members` |
| Zone resources | `/zones?account.id={account_id}` |

The first four endpoints accept Account Settings Read; zone listing needs Zone
Read (rendered `Zone Zone Read` in the API reference). See [account details](https://developers.cloudflare.com/api/resources/accounts/methods/get/),
[members](https://developers.cloudflare.com/api/resources/accounts/subresources/members/methods/list/),
[IAM user groups](https://developers.cloudflare.com/api/resources/iam/subresources/user_groups/methods/list/),
[group members](https://developers.cloudflare.com/api/resources/iam/subresources/user_groups/subresources/members/methods/list/),
and [zones](https://developers.cloudflare.com/api/resources/zones/methods/list/).

Account keys are `member:ACCOUNT_ID:MEMBERSHIP_ID`, because IAM group member IDs
identify account memberships. A member's email is a display login only; verified
emails and authoritative identities are not emitted. Unknown principal kind is
retained. Accepted membership is required before a principal or membership path
is emitted. Pending invitations are excluded; malformed/absent status marks the
snapshot partial. Provider instance IDs namespace every graph key.

Group keys are `user-group:ACCOUNT_ID:GROUP_ID`. Permission groups inside policies
are native roles, not membership groups. Group access is collected separately:
Cloudflare notes that member role fields do not include inherited user-group
permissions. See [user-group semantics](https://developers.cloudflare.com/fundamentals/manage-members/user-groups/).

Grants preserve the native permission-group name, `observed` certainty and
`unknown` privilege. Their provenance explicitly identifies member or user-group
policy assignment. They establish the observed assignment, not fully evaluated
effective access. This adapter does not answer effective-administrator questions.

A policy is normalized only when its account scope exactly matches configuration
and its objects are either a known visible zone or the documented `*` object.
An exact zone produces an assignment to `zone:ZONE_ID`. A wildcard produces a
separate `policy-scope:account:ACCOUNT_ID:all` evidence resource; it is never
expanded into assumed account-wide or per-zone effective grants. Unknown scope
forms, new fields on policies/resource groups/permission groups/scopes and
conflicting role definitions are rejected.

The core graph has no deny-effect field. A deny or unsupported policy suppresses
that subject's entire grant set. For affected members, membership paths are also
suppressed; a denying group blocks other access paths for its known members.
This deliberately favors omission over falsely reporting access. Cross-account
zones, conflicting duplicate records and unresolved memberships are excluded
with partial warnings. Policy-less/legacy-role-only records are not promoted to
assumed scope; legacy role-list endpoints are not called.

## Bounds, partial results and privacy

`complete=true` means the planned bounded collection finished. It does not prove
all access was visible. Permanent limitations cover token scoping, pending
invitations, nontransactional reads, organization policy, token principals,
Zero Trust Access rules and non-zone product resources. Failed list pages retain
previous useful observations with `complete=false`. Failure to read the configured
account returns an error. An operation-wide timeout or graph expansion limit also
returns an error because an unfinished graph is not validated output.

Discovery uses 50 rows/page, at most 100 pages/list, 20,000 source rows, 2,000
HTTP attempts and a 20,000 bound on accumulated assignment expansion (and combined
group-stage assignments/memberships). Each response is capped at 2 MiB, including
streamed bodies. Connection timeout is five seconds; request and body timeout is
15 seconds; the library operation timeout is 120 seconds. The subprocess host may
apply a tighter deadline. Dropping the future cancels requests and retry sleeps.

Pagination follows local numeric pages, honors both total_count and total_pages
(including short intermediate pages), checks response counts, rejects repeated
pages and contradictory or changed totals, and probes another page if a full
response omits totals. Prior totals remain binding if a later response omits them.
No server-provided next URL is requested. Successful HTTP responses also require
`success: true`. Two retries at most handle 429/5xx responses. Numeric Retry-After
is honored up to 30 seconds; larger/malformed delays or 429 without a delay return
a rate-limit failure. Other transient retries use bounded exponential delay.

Cloudflare documents 1200 requests/five minutes per user/account token and
200 requests/second per IP; user usage can include dashboard traffic. See
[rate limits](https://developers.cloudflare.com/fundamentals/api/reference/limits/).

Errors never copy raw bodies, URLs, headers, tokens or transport diagnostics.
Protocol output uses sanitized limitation/error categories and the shared native
runtime. Test-only loopback origins are private to unit-test code.

## Validation

Run `cargo test -p permesh-provider-cloudflare` and
`cargo clippy -p permesh-provider-cloudflare --all-targets -- -D warnings`.
Synthetic HTTP and contract tests cover normalization, denies, pending members,
exact/wildcard/unknown scopes, cross-account data, conflicts, pagination, partial
failures, redirects, throttling, response limits, deadlines and cancellation.
Native subprocess tests validate setup, credential boundaries and cancellation.
Live validation and five-target release qualification remain required before
publishing an installable catalog entry.
