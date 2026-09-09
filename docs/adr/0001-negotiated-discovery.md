# Adopt negotiated v1 discovery without a legacy projection

Status: accepted for unpublished 0.2.0 candidates.

The host now separates its internal domain from independent wire DTOs. Its richer
identity dimensions and access evidence cannot be represented faithfully by the
experimental discovery schema. Silently dropping them would give different
security answers depending on the execution path.

Official source providers therefore accept negotiated v1 for health/discovery
and reject experimental discovery requests before invoking an adapter. Explicit
DTO mapping and snapshot validation keep internal refactors from changing the
public wire shape. Setup and browser authentication retain their existing
contracts. The existing native subprocess, sanitized environment, credential
scoping, timeouts and trust boundaries remain unchanged.

This is a pre-adoption breaking change with a 0.2.0 candidate package version.
Published binaries are immutable. Host installation and workspace selection must
be qualified before catalog publication. See [adoption and evidence semantics](../negotiated-v1.md).
Future compatible operations should use capability negotiation; version numbers
represent wire compatibility, not the number of implemented features.
