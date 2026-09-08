# Changelog

## Unreleased

- Project native discovery records and handshake capabilities onto the frozen
  protocol schema before serialization. Existing draft versions and emitted
  bytes remain unchanged; internal domain model additions cannot silently
  extend the wire records.

- Return the protocol-defined `unsupported_method` error when a native provider
  has no browser authentication description. GitHub, Cloudflare and AWS now
  report a valid unsupported operation instead of a malformed protocol response.
