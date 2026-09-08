# Changelog

## Unreleased

- Exclude conflicting GitHub account, repository, team and role observations
  with dependent claims, and mark collection incomplete. Identical repeated
  observations retain their existing stable keys and deduplicate normally.

- Return the protocol-defined `unsupported_method` error when a native provider
  has no browser authentication description. GitHub, Cloudflare and AWS now
  report a valid unsupported operation instead of a malformed protocol response.
