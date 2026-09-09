# Changelog

## Unreleased

- Breaking, unpublished 0.2.0 candidates: discovery and health now require the
  negotiated `protocol_version: 1` envelope and matching operation. Experimental
  draft 2 invocations are rejected before adapter construction. Setup draft 3
  and browser authentication draft 4 retain their existing contracts.
- Emit independent v1 wire DTOs with identity kind, affiliation and lifecycle,
  provider-owned resource kinds/parents, and access evidence. Validate snapshots
  before emitting records; preserve observed assignments without claiming
  effective authorization. See [migration requirements](docs/negotiated-v1.md).
- Qualify both negotiated operations through offline native handshake/cancellation
  smoke tests, without credentials or provider API requests.
- Exclude conflicting GitHub account, repository, team and role observations
  with dependent claims, and mark collection incomplete. Identical repeated
  observations retain their existing stable keys and deduplicate normally.

- Return the protocol-defined `unsupported_method` error when a native provider
  has no browser authentication description. GitHub, Cloudflare and AWS now
  report a valid unsupported operation instead of a malformed protocol response.
