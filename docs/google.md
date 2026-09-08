# Google Workspace Directory provider

The native `google` executable provides **accounts and identities** through
protocol draft 2 discovery/health and draft 3 declarative setup. Source and local
qualification are available; no Google package is published or cataloged yet.
The existing GitHub 0.1.0 release assets and catalog entries are unchanged.

## Configuration and authentication

`customer_id` is the explicit stable Workspace customer ID beginning with `C`.
`my_customer`, custom API/token endpoints and unknown settings are rejected.
The setup description asks for one of these modes:

| Configuration | Named credentials | Behavior |
| --- | --- | --- |
| `customer_id`, optional `auth_mode: access_token` | `token` | Uses the supplied access token; remains compatible with migrated configurations. |
| `customer_id`, `auth_mode: refresh_token`, `client_id` | `refresh_token`, `client_secret` | Exchanges the refresh credential once per invocation, then uses the resulting access token. |

The OAuth client ID is nonsecret. Supply credentials through `env://NAME` or
`keychain://INSTANCE/SLOT` references in the host, never literal secret values in
workspace configuration. The provider receives only the selected named slots
through the host protocol; it does not read files, environment or arguments.

For either mode, enable the Admin SDK API and authorize an administrator with
permission to read users for this customer. Request the minimal scope
`https://www.googleapis.com/auth/admin.directory.user.readonly`, which grants
retrieval of users and aliases. This adapter requests only primary email and
status fields. See Google's [Directory scope documentation](https://developers.google.com/workspace/admin/directory/v1/guides/authorizing)
and [users.list reference](https://developers.google.com/workspace/admin/directory/reference/rest/v1/users/list).

For refresh mode, create an OAuth client and obtain an offline refresh token
using Google's supported authorization flow for that client type. The current
provider accepts an already-issued refresh credential; it does not open a browser
or implement an authorization-code receiver. Google's [installed-app flow](https://developers.google.com/identity/protocols/oauth2/native-app)
documents PKCE and the refresh request's client ID, client secret, refresh token,
and `grant_type=refresh_token`. Supply the issuing client's secret even for a
desktop client, where Google notes it cannot be treated as confidential.

The only token endpoint is `https://oauth2.googleapis.com/token`. Exchanges use a
single HTTPS POST without redirects or retries, with 5-second connection and
15-second total deadlines. Token response bodies are bounded to 64 KiB and token
values to 16 KiB. Application-owned encoded request and response buffers are
zeroized when dropped; HTTP/TLS libraries manage their own transport buffers.
Access tokens are kept only for the invocation, never cached or written to disk.
Refresh is cancellable along with discovery. Remote error bodies, credentials,
URLs and transport diagnostics are not copied into protocol errors or stderr.
Records containing an access-token reflection in IDs or primary email are
excluded before normalization, including tokens newly issued by the refresh
endpoint that the host itself has never seen.

Refresh credentials can expire or be revoked. Google documents seven-day expiry
for many externally tested OAuth apps, plus other lifetime limits in its
[OAuth overview](https://developers.google.com/identity/protocols/oauth2).
Revoke the authorization in Google account security settings or through your
administrator; deleting a local reference does not revoke Google's token.

## Observations and limitations

The only data endpoint is
`https://admin.googleapis.com/admin/directory/v1/users`; requests are GET with an
explicit customer, `projection=basic`, `viewType=admin_view` and the fields
`id,customerId,primaryEmail,suspended,archived` plus pagination metadata.

Account keys retain the configured instance and raw Google user ID. Canonical
identity IDs remain `google:CUSTOMER_ID:USER_ID`, including across email and
instance renames. Only `primaryEmail` is directory-attested for exact email
correlation. Aliases and recovery addresses are excluded. Directory attestation
does not prove mailbox ownership or employee status.

Identity kind is unknown. Status is inactive if suspended or archived is true,
active only when both fields are explicitly false, and otherwise unknown.
Mismatched customers and invalid records are excluded; duplicate native IDs
remove all conflicting claims. Later-page errors retain earlier records and mark
the result incomplete. Health probes at most one user, without establishing full
visibility. Groups, memberships, resources, grants and deleted users are absent.
Collection is not transactional and always carries visibility limitations.

API limits are 2 MiB per response, 500 users per page, 200 pages, 100,000 source
rows and 15 seconds per request. GET quota/5xx responses allow two retries with
bounded delay; ordinary permission failures do not retry. Pagination tokens are
encoded as query values, never followed as URLs. Redirects are disabled.

The shared native runtime imposes a 55-second operation deadline, 1 MiB NDJSON
frames, 64 MiB input/output transcripts and 100,000 total output records. Since
each Google user yields an account and identity, the external protocol permits
at most 50,000 paired users; larger results fail before record emission. Host
limits can be stricter. Cancellation drops active requests and retry waits.

## Source provenance and qualification

The API adapter and original HTTP contract tests were copied from Permesh commit
`929ae3e2c69d0cd4cbfb2fdbfce6ed3085f36f69` without changing native IDs, identity
status or primary-email semantics. Their MIT terms and copyright are retained in
[the repository license](../LICENSE). SDK/core/protocol/secrets
remain pinned to the workspace's existing public revision. Shared protocol
framing, request validation, deadlines, cancellation and output serialization are
factored from the proven GitHub executable into `permesh-native-runtime`.

Run `cargo test --workspace --locked` and the Python packaging tests. Synthetic
HTTP fixtures cover Google API semantics, refresh form encoding, bounded/redacted
OAuth failures, redirect rejection and cancellation. Protocol tests cross-decode
health/discovery/setup through the host decoder. Native process tests cover
malformed input, no secret echo, queued cancellation and stdin-open shutdown.
No live tenant credentials are used by automated tests.

For a locally built candidate:

```sh
cargo build --release --locked -p permesh-provider-google --target aarch64-apple-darwin
python3.12 scripts/smoke_provider.py --provider google target/aarch64-apple-darwin/release/permesh-provider-google
python3.12 scripts/package_provider.py --provider google --target aarch64-apple-darwin --output google-candidate
python3.12 scripts/verify_package.py google-candidate/*.zip --entry google-candidate/catalog-entry.json
```

Use your actual supported native target. Candidate workflows qualify both providers
on the five existing target platforms. Building a candidate does not publish a
release or add it to the catalog.

After reviewing the candidate bytes and their SHA-256, register the executable
and inspect its setup form:

```sh
permesh provider external trust /absolute/path/to/permesh-provider-google \
  --id google --sha256 REVIEWED_SHA256 \
  --capability accounts --capability identities --accept-risk
permesh provider setup google --id directory --describe
```

For scripted refresh-token setup, put only references in the answer file:

```yaml
version: 1
answers:
  customer_id: C123
  auth_mode: refresh_token
  client_id: YOUR_OAUTH_CLIENT_ID
  refresh_token: keychain://directory/refresh_token
  client_secret: keychain://directory/client_secret
```

```sh
permesh provider setup google --id directory --answers answers.yaml --authoritative
permesh auth login directory --credential refresh_token
permesh auth login directory --credential client_secret
permesh provider external review directory
permesh provider external approve directory --fingerprint REVIEWED_FINGERPRINT --accept-risk
permesh doctor
```

Trust and workspace approval are separate explicit decisions. Native providers
run with your user permissions. Preserve the existing instance ID when migrating
an existing built-in directory; do not create a duplicate instance just to change
its implementation. Setup never resolves credentials or approves the workspace.

Use `--authoritative` only when this directory is a reviewed identity source of truth. Omitting it still discovers accounts, but the CLI will not treat directory status as authoritative for orphan review. Migration preserves the original authority declaration rather than choosing one automatically.

## Browser login (0.1.1 source)

The 0.1.1 source adds an optional draft 4 authentication description. It requires
an updated CLI with `auth login --browser`; the 0.1.0 provider and CLI alpha.1 do
not implement that flow. Draft 2 discovery and draft 3 setup remain unchanged.
Catalog protocol entries describe discovery/setup compatibility; optional browser
authentication is negotiated separately and fails closed on older executables.

Create your own **Desktop app** OAuth client in Google Cloud, enable the Admin SDK,
and configure its consent audience for the Workspace administrators who will use
Permesh. Permesh has no shared OAuth client or backend. Google documents client
creation and the loopback/PKCE flow in its [native-app guide](https://developers.google.com/identity/protocols/oauth2/native-app).

Use refresh-token setup with that client's ID, a `client_secret` reference, and
`keychain://directory/refresh_token` as the refresh-token reference. Store the
client secret through the existing hidden credential prompt. Review and approve
the exact workspace and executable before requesting browser authentication:

```sh
permesh auth login directory --credential client_secret
permesh provider external review directory
permesh provider external approve directory --fingerprint REVIEWED_FINGERPRINT --accept-risk
permesh auth login directory --browser
permesh doctor
```

The provider declares only Google's fixed authorization/token endpoints, the
Directory user-readonly scope, and references to existing configuration fields
and credential slots. The CLI owns the local callback, state and PKCE validation,
code exchange and keychain write. No credential is sent to the provider while
requesting its authentication description. Ordinary access queries neither open
a browser nor write a new refresh credential. Access-token-only configuration is
not silently converted; configure refresh mode explicitly first.

Google's consent screen and account policy still determine whether authorization
is allowed. App verification, test-user restrictions and administrator consent
remain Google-side prerequisites. Local synthetic OAuth tests are not a completed
live Google authorization or tenant-visibility qualification.
