# Negotiated v1 source candidates

The unpublished 0.2.0 providers use the negotiated v1 discovery contract in
Permesh revision `0196197a215c0b244251fe1e3ca1eb759159776b`. The current release
catalog still describes older published binaries. Updating source does not
change those binaries, install new code, or approve a workspace.

## Compatibility and adoption

Discovery and health use `protocol_version: 1`, a handshake naming the requested
operation (`check` or `discover`), and a matching configured invocation. The
handshake advertises supported operations separately from record capabilities.
Experimental discovery envelopes using `protocol: 2` are rejected before adapter
construction or provider API access. There is no lossy legacy projection.

Setup descriptions still use `protocol: 3`; browser authentication descriptions
still use `protocol: 4`. These distinct experimental operations have not been
renumbered by this change. Negotiated v1 remains a draft: this rollout does not
declare protocol stability. Compatible future operations belong in negotiation,
not an automatic protocol-version increment.

For a locally registered candidate, retain the existing external provider
configuration and select:

```yaml
external:
  discovery_protocol: negotiated_v1
```

This is a field excerpt, not a complete provider configuration. Use a compatible
host, review the candidate executable and its digest, then review and approve
its workspace context before running it with credentials. Changing either the
binary or this selector invalidates the corresponding trust/approval. Old
published 0.1.x binaries keep their existing legacy workflow.

**Publication gate:** the official installation/setup path must explicitly select
and validate the new discovery contract before 0.2.0 packages enter the catalog.
The pinned host selects the discovery contract from verified catalog metadata
for guided official setup. Standalone setup and explicit external configuration
retain the legacy default unless a selector is supplied. Older hosts that do not
recognize this metadata are incompatible with these candidates.
Release qualification must exercise install, trust, setup, approval and both
operations on the matching host. No release or catalog update is part of this
source migration.

Candidate `catalog-entry.json` now includes `discovery_protocol: negotiated_v1`
and lists only legacy setup in `protocols: [3]`. The pinned Permesh revision
accepts and validates this metadata, as well as historical legacy entries. The
archive verifier and matching host reject combined negotiated and legacy
discovery claims. Candidate metadata is not evidence that an artifact has been
published or qualified: validate the actual release through the matching host
and the publication gate above before adding it to the catalog.

## Preserved observations

- GitHub distinguishes API-reported users and bots while leaving lifecycle and
  affiliation unknown. Repositories have observed organization parents where
  available. Team assignments retain their path and native permission name.
- Google Directory represents lifecycle independently of kind and affiliation:
  archived users are inactive, otherwise suspended users are suspended, and
  explicit non-archived/non-suspended users are active. Without a positive lifecycle flag, missing evidence remains
  unknown; malformed lifecycle observations make discovery incomplete.
- AWS IAM policy attachments are observed policy-attachment evidence. Their
  effective privilege remains unknown; the IAM binary does not enumerate Identity
  Center or Organizations.
- Cloudflare preserves role and scoped assignment observations, with account and
  zone resource kinds and observed hierarchy. Effective privilege remains unknown.

- GitLab preserves direct and separately labeled collapsed effective membership
  within approved groups/projects; it does not invent inheritance paths.
- Entra preserves tenant/object IDs and direct directory memberships; explicit
  host authority/mappings are required and UPN is not verified email.
- The separate Identity Center binary preserves provisioned permission-set
  assignments for an explicit account allowlist, always with unknown privilege.
  Optional Organizations reads only decorate allowlisted account names.

The runtime projects records into explicit public wire DTOs, validates the
snapshot and instance boundary before emitting records, and sorts output.
Completeness and known limitations remain part of discovery. Each new candidate
guide lists its implemented read APIs and required permissions.

## Qualification

Adapter HTTP fixtures exercise normalization and cross-decode the emitted
records with the pinned Permesh host decoder. Runtime tests cover framing,
operation selection, invalid snapshots and cancellation. Native candidate jobs
check setup and both negotiated handshakes without invoking APIs or supplying
credentials. Offline success is not a live-provider qualification claim.

The newer GitLab, Entra and Identity Center binaries have local synthetic/native
checks; historical five-target runs do not qualify their new bytes. See the
[qualification matrix](qualification.md) and [build guide](development.md).
