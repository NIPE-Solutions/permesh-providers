# Read-only live acceptance procedure

This is a procedure, not evidence that live checks passed. Start from the
[qualification matrix](qualification.md). Running it requires separate operator
authorization for the chosen test tenant and credentials. Offline preparation
needs no credentials and must not resolve secret references.

## Prepare an exact candidate

1. Record the CLI revision/version, provider revision/version, native target/OS,
   candidate run URL and archive/executable SHA-256. Preserve the original archive
   in restricted release evidence storage. Check the package inventory, native
   target, licenses and both digests using the existing package verifier/store.
   Checksums establish integrity against reviewed metadata; provenance, publisher
   signatures, platform signing and notarization are separate checks.
2. Use the published GitHub 0.1.0 path only with its legacy contract. For an
   unpublished candidate, follow [development setup](development.md) and
   [negotiated adoption](negotiated-v1.md): build/inspect exact bytes, explicitly
   trust the executable, then obtain its setup description. Do not insert a local
   candidate into the release catalog or describe it as installable.
3. Verify advertised record capabilities, legacy setup draft and negotiated
   check/discover operations against metadata. On a matching host, complete setup
   with secret references only; select `negotiated_v1` for candidates. Review the
   instance, tenant, optional authority, aliases, native digest and network policy,
   then explicitly approve the fingerprint. Do not reuse approval after edits.
4. For a portable map, verify all entries belong to one exact version and matching
   contracts. Each platform operator must independently inspect/trust its own
   native artifact and approve its workspace. Installing/updating a package must
   leave existing pins and approvals unchanged.

## Authorized tenant checks

Use a disposable or explicitly approved test tenant. Keep raw reports and tokens
out of repository files, CI logs, command arguments and shell traces. Host-owned
environment or keychain references deliver only declared slots after approval and
handshake. Record scope names and pass/fail findings, not credential values,
customer inventory or identifiable user/resource labels.

| Provider | Positive read checks | Visibility / negative checks |
| --- | --- | --- |
| GitHub | Health with active org membership; organization role, one visible repository, known team path and collaborator role; compare stable numeric IDs and native roles with operator-reviewed console/API facts | Use a separately authorized reduced-scope token or preexisting limited fixture; ensure hidden repos/failed team lookup are limitations, not proof of absence. Classic `repo` may grant writes although adapter uses GET. Do not alter org membership to manufacture a test |
| Google | Access-token mode, then refresh mode if separately authorized; customer-bound users, primary-email correlation and preexisting archived/suspended users. If desktop flow is in scope, explicitly run authorized browser login and verify named keychain storage without displaying token values | Wrong customer and insufficient read scope fail honestly. Compare preexisting missing/malformed status fixture behavior offline. Never archive/suspend real users for this test. Browser flow with explicit network settings must reject; service-account JWT is unsupported |
| Cloudflare | Health, accepted members, known IAM group membership, visible zone, exact/wildcard assignment with native role retained and privilege unknown | Validate preexisting pending membership is not an accepted access path; denied/unsupported policies cannot create false allow paths. Reduced zone/group visibility remains partial. Do not create/revoke API tokens or change policies through the adapter |
| AWS IAM | STS confirms intended account; compare known immutable user/role/group/policy IDs and managed/inline attachments from an approved IAM inventory | Wrong account rejects. Use preexisting limited credentials to inspect permission failures if authorized. Attachments must not become effective-admin claims; no Identity Center, session, key, root or service-resource inventory is implied |

Run `permesh doctor --details` for the same health probes, then the appropriate
`user`, `admins` and `orphaned` queries. Health success is not discovery success.
For the reference cross-provider workflow, explicitly approve Google as an
identity source and map a known GitHub stable numeric account ID to the canonical
`google:CUSTOMER:USER` identity. Compare a known team-derived access path and a
separate observed collaborator permission; do not call the latter a proven direct
assignment. Confirm inactive identity access remains visible, bots remain bots,
and unmapped accounts are unresolved. Google does not establish service kind.

Use offline fixtures for destructive-condition simulations: inaccessible source,
renamed/recycled login, denied later page, partial authority, revoked local
approval, changed executable and cancellation. One source failure must preserve
other valid observations with incomplete status; incomplete authority must not
turn an account into a confidently orphaned identity. No test may deliberately
change live permissions merely to generate these cases.

If approved network policy is required, qualify a real authorized proxy and CA
on each required target; verify the allowed bypass route. AWS must reject the
feature without receiving credentials. Verify incompatible feature/contract
negotiation withholds invocation data using offline peers, never captured real
credential transcripts. Keep TLS hostname verification enabled.

## Evidence and release decision

Store a redacted record outside source control with these fields:

- Date, operator/reviewer, exact CLI/provider revisions and versions, OS/target,
  artifact hashes and source workflow URL.
- Test tenant scope described without real inventory; API product/version,
  credential mode, permission/scope names, approval and network configuration class.
- Each planned check: passed / failed / not run / unsupported, observed limitation,
  expected vs actual outcome and a redacted evidence reference.
- Separate decisions for native package validation, host compatibility, desktop
  credential UX, API live acceptance and release/catalog readiness.

A missing credential, inaccessible platform or unsupported scope is **not run**
or **unsupported**, never a pass. Do not qualify new bytes using an older release's
live result. Resolve failures or record an explicit restricted supported scope
before a maintainer considers publication. Publishing archives, updating catalog
entries, revoking upstream credentials and altering tenant state remain separate
operator actions; this procedure performs none automatically.
