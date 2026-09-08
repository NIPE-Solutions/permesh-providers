# Contributing

This repository is establishing official provider distribution. Start with the
[distribution contract](docs/distribution.md) and
[provider lifecycle](docs/provider-lifecycle.md). Installation and update commands
belong in the [Permesh CLI repository](https://github.com/NIPE-Solutions/permesh).

Discuss a provider's API coverage and permission model before adding it. Keep
changes focused, preserve stable provider identifiers and inheritance paths, and
report uncertainty where an API cannot prove effective access. Provider code must
not print presentation output or perform access mutations.

Use synthetic fixtures and local mock servers. Never commit credentials, private
organization metadata, access reports or live API payloads. Ordinary tests must
not depend on live credentials. Each implementation needs provider contract tests,
API error and pagination coverage, documentation and cross-platform validation.
There is no build command for this documentation-only foundation yet; a provider's
build and test instructions must arrive with its implementation.

Submit changes through pull requests describing the concrete behavior, tests and
observable limitations. Review catalog changes as software distribution changes:
verify exact versions, origins, capabilities, checksums and compatibility. Do not
list a provider before its artifacts are qualified.

Contributions use MIT OR Apache-2.0. Retain original attribution and required
third-party notices when moving code. See [LICENSE](LICENSE).
