# Changelog

## Unreleased

- Return the protocol-defined `unsupported_method` error when a native provider
  has no browser authentication description. GitHub, Cloudflare and AWS now
  report a valid unsupported operation instead of a malformed protocol response.
