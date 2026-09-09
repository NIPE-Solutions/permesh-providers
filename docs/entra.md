# Microsoft Entra provider candidate

`permesh-provider-entra` 0.2.0 is a read-only public-cloud Microsoft Graph candidate.
It is not in the public catalog and has not been live-tenant qualified. It collects
one explicitly configured tenant's directory identities/accounts, groups and direct
user/group memberships. It does not evaluate authorization or infer employment.

## Configuration, credentials and tenant proof

Configuration accepts `tenant_id` (a UUID) and optional
`include_service_principals` (boolean, default false). Credentials accept one named
`token` slot. Supply an approved secret reference such as `env://ENTRA_GRAPH_TOKEN`
or a named keychain reference through the host; never put the access token in
configuration or command-line arguments. The setup form does not create credentials.

Before discovery, and on every health check, the adapter requests
`GET https://graph.microsoft.com/v1.0/organization?$select=id`. The response must
contain exactly one organization with the configured tenant UUID and no continuation.
A mismatch or failed proof stops enumeration. This uses the authenticated Graph
response as tenant evidence; it does not decode an unverified JWT. A successful
health check proves this tenant response only, not every discovery permission.
See [organization API](https://learn.microsoft.com/en-us/graph/api/organization-list?view=graph-rest-1.0).

The documented sufficient **read-only application-permission baseline** is:

| Permission | Implemented use |
|---|---|
| `Organization.Read.All` | Authenticated tenant proof |
| `User.Read.All` | User directory listing and selected lifecycle properties |
| `GroupMember.Read.All` | Group listing and direct membership |
| `Application.Read.All` | Only when service-principal inventory is enabled |

Application permissions require administrator consent. Use a token issued for
Microsoft Graph in the reviewed tenant. No token acquisition, OAuth refresh,
certificate credentials, managed-identity credential discovery or ambient cache
lookup is implemented. Token expiry requires supplying a fresh credential through
the same approved reference. The adapter never requires write permissions.

This is a sufficient combined read baseline, not a claim of absolute minimum scope
for every token type or newest Graph permission combination. Current membership docs
also list `GroupMember.ReadBasic.All`; group-list documentation lists different
permission combinations. Narrower split basic permissions and delegated-user roles
need separate live qualification. Personal Microsoft accounts are unsupported.
Hidden membership additionally requires `Member.Read.Hidden`, which is not requested
by default; denied hidden-member reads remain partial.

## Exact API subset

Only GET requests target the fixed `https://graph.microsoft.com/v1.0/` origin.
National-cloud endpoints, `/beta`, custom origins, directory role grants, app-role
assignments, Azure resource RBAC, PIM, Conditional Access, sign-in/session/token
inventory and invitation APIs are not implemented.

| Collection | Endpoint and selected properties |
|---|---|
| Users | `/users?$select=id,displayName,userPrincipalName,accountEnabled,userType&$top=100` |
| Groups | `/groups?$select=id,displayName&$top=100` |
| Optional workload principals | `/servicePrincipals?$select=id,displayName,accountEnabled,servicePrincipalType&$top=100` |
| Direct members of each observed group | `/groups/{object_id}/members`, default Graph page size |

References: [users](https://learn.microsoft.com/en-us/graph/api/user-list?view=graph-rest-1.0),
[groups](https://learn.microsoft.com/en-us/graph/api/group-list?view=graph-rest-1.0),
[service principals](https://learn.microsoft.com/en-us/graph/api/serviceprincipal-list?view=graph-rest-1.0),
[direct group members](https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0).

The v1 group-members API has a documented service-principal omission. This candidate
excludes service-principal membership edges entirely, even if some are returned;
optional service-principal **objects** remain useful workload identity inventory.
It does not silently switch to beta or assume `$expand` is completely paginated.
Completeness is within the declared user/nested-group membership scope, not all
possible group member types. Actually returned devices, organizational contacts,
unknown types or missing referenced directory objects mark collection partial.

Direct nested-group edges remain direct membership evidence. They do not become
transitive membership rows or permission grants. No resource or grant capability is
advertised. Group display names do not imply privileged roles. Restricted/missing
properties retain unknown values; wrong typed properties reject the page as malformed.

## Stable identities and lifecycle

Native object UUIDs and the proved tenant UUID form every identity:

- Canonical identity: `entra:TENANT_UUID:OBJECT_UUID`.
- Account key: provider instance plus `object:TENANT_UUID:OBJECT_UUID`.
- Group key: provider instance plus `group:TENANT_UUID:OBJECT_UUID`.

UUIDs are normalized to lowercase. Renaming a display name or UPN does not change
these keys. Users have unknown principal kind: a directory user object does not
prove a human. `Guest` yields external **directory** affiliation, not an employment
conclusion; `Member` remains unknown affiliation. `accountEnabled: false` is inactive,
true is active, and absent/null is unknown. This is account lifecycle, not session
revocation or employment status.

Recognized `Application`, `ManagedIdentity` and `Legacy` service-principal types yield
service kind; future/missing classifications remain unknown. Workload affiliation is
unknown. Only selected service-principal object properties are exposed, not application
credentials, all application registrations or effective app permissions.

UPN and display name are labels. No verified email is emitted. Directory authority is
an explicit host configuration choice. The current host also requires explicit stable
account mappings to associate these accounts with the emitted canonical identities;
equal-looking labels or IDs do not automatically bind an account. For example, map
account `object:TENANT_UUID:OBJECT_UUID` from the configured provider instance to exact
canonical identity `entra:TENANT_UUID:OBJECT_UUID` using the host identity workflow.
The synthetic host-query test covers both the unbound and explicitly bound cases.

## Network, limits and failure behavior

The candidate opts into the existing `network_v1` feature without introducing a
protocol version. It uses the shared HTTP builder for explicit proxy/no-proxy and
additional CA roots, preserves platform trust, disables redirects and inherits no
process proxy environment. All requests, including tenant proof, use that client.
No TLS verification-disable option exists. Negotiated check/discover use protocol
version 1; setup retains the runtime's separate legacy setup contract.

Bounds are 5 seconds to connect, 10 seconds per request, 45 seconds per operation,
2 MiB per response, 999 rows per page, 100 pages per collection, 50,000 rows across
collections and 1,000 HTTP attempts per operation. Tokens are at most 16 KiB; selected
string fields are bounded to 4 KiB. These are transport/record bounds, not peak-memory
claims. A 429 needs an integer `Retry-After` no greater than 10 seconds; up to three
attempts are made. Bounded server-error retries use the same operation deadline.

Opaque next links must preserve exact origin, path and initial selection/page size.
Only `$select`, `$top` and `$skiptoken` query keys are supported; credentials in URLs,
fragments, changed selectors, duplicated parameters, repeated URLs/IDs and unrelated
hosts are rejected. No pagination URL or API error body becomes an error message.

Validated earlier pages and successful collections survive later 401/403, malformed
responses, unsupported pagination, throttling or budget exhaustion, with static
partial limitations. An operation deadline or failed tenant proof returns a typed
provider failure. A failed call or hidden object never establishes deactivation or
absence of access. Graph consistency and token visibility remain permanent qualifiers.

## Qualification and authorized live acceptance

Automated checks exercise synthetic Graph responses, scoped tenant proof, disabled
and guest users, optional workloads, direct nested groups, known omitted/unknown
member variants, restricted attributes, pagination, retries, redirects, bounds,
malformed input, secret-free diagnostics, explicit host identity mapping, negotiated
host cross-decoding, setup and subprocess cancellation. A real subprocess CONNECT
fixture proves explicit proxy routing while hostile ambient proxy settings are ignored;
its Graph destination is never contacted. Shared-runtime tests separately qualify CA
parsing, positive private-CA TLS and hostname validation.

Native validation in this slice is local only. Five-target artifact publication,
installation/catalog metadata, live permission consent and tenant/API behavior remain
unqualified until recorded separately. No compatible published release is claimed.

Before an explicitly authorized live session:

1. Review the exact tenant, candidate binary digest, authority choice, read scopes and
   optional service-principal flag. Supply no credentials until host trust and approval
   cover that configuration and network context.
2. Provision a reviewed Graph token outside this adapter. Confirm administrator consent
   for the selected application permissions; do not add writes to satisfy a read test.
3. Run check and independently compare its configured tenant with the approved tenant.
   Wrong-tenant credentials must fail before directory listing.
4. Discover a deliberately small test tenant with enabled/disabled users, a guest,
   nested groups and an optional workload principal. Compare native IDs and direct
   memberships to independent Graph/admin observations; record missing/restricted data.
5. Exercise pagination and a separately authorized restricted token or hidden group.
   Record partial state and retained valid evidence; never treat denial as removal.
6. Configure explicit authority and account mappings. Confirm renamed labels retain
   stable IDs and UPN alone does not create a binding. Save only explicitly approved
   sensitive reports, without tokens or raw credential-bearing diagnostics.
7. Record exact API date, native platform, binary/source digest, permission set,
   unsupported scopes and observed results. Until these steps are run, live status is
   **not run**, not passed. Revoke test credentials through the appropriate admin system.
