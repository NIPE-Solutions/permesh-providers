# Permesh providers

Official providers for [Permesh](https://github.com/NIPE-Solutions/permesh).

**Know who has access to what.**

This repository is the home for independently distributed, read-only Permesh
providers and their release catalog. Providers discover access metadata directly
from the services users configure. Permesh has no backend or telemetry.

## Status

[GitHub provider 0.1.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/github-v0.1.0)
is available for macOS Apple Silicon and Intel, Linux GNU ARM64 and x86_64, and
Windows x86_64. The [catalog](catalog/v1.json) lists the qualified packages.

```bash
permesh provider install github --version 0.1.0
permesh provider update github --check
permesh provider update github
```

Continue with [binary trust, setup and workspace approval](docs/github-setup.md).
Existing built-in GitHub configurations have an explicit
[migration path](https://github.com/NIPE-Solutions/permesh/blob/main/docs/github-migration.md).

Updates are requested by the user. Ordinary access queries never check for
updates. Downloading a package does not execute it, resolve credentials, or
approve a workspace. Version selection and workspace adoption are separate
operations; see the [distribution contract](docs/distribution.md).

Google Workspace Directory is available as an unpublished native candidate with
access-token and refresh-token authentication. See [Google setup, permissions and
coverage](docs/google.md).

AWS IAM attachment inventory is available as an unpublished native candidate with
explicit access-key and optional session-token references. Attachments retain
unknown effective privilege. See [AWS source](providers/aws) and
[authentication, permissions and limits](docs/aws.md). Live qualification and
catalog publication remain pending.

## Current source compatibility

The 0.2.0 source candidates are unpublished. They use negotiated protocol v1
for discovery and health, preserving identity lifecycle, resource hierarchy and
access evidence. They require a compatible host and explicit workspace opt-in;
see the [upgrade guide](docs/negotiated-v1.md). The published packages and catalog
above remain unchanged.

## Repository responsibilities

- Provider source, synthetic fixtures, API contract tests and limitations.
- Provider-owned setup descriptions with CLI-owned prompts.
- Independently versioned native releases and reviewed catalog metadata.
- Authentication and minimum read-permission documentation for every provider.

The CLI, shared SDK, domain model and subprocess protocol remain in the
[Permesh repository](https://github.com/NIPE-Solutions/permesh).
Third-party and internal providers can use that protocol without joining this
repository or using Rust.

## Documentation

- [AWS IAM authentication and attachment evidence](docs/aws.md)
- [Google Directory authentication and coverage](docs/google.md)
- [Cloudflare IAM observations and limits](docs/cloudflare.md)
- [GitHub permissions and coverage](docs/github.md)
- [GitHub installation and setup](docs/github-setup.md)
- [Dependency policy](docs/dependencies.md)
- [Release qualification](docs/releasing.md)
- [GitHub 0.1.0 qualification record](docs/releases/github-0.1.0.md)
- [Catalog schema and archive layout](docs/catalog.md)
- [Distribution and explicit updates](docs/distribution.md)
- [Provider migration and release acceptance](docs/provider-lifecycle.md)
- [Contributing](CONTRIBUTING.md)
- [Security reporting](SECURITY.md)

## License

MIT. See [LICENSE](LICENSE). Third-party dependencies retain their own licenses
and required notices in distribution archives.

Cloudflare account members, IAM groups and scoped policy assignments are implemented as a native provider. Source and mock/protocol tests are available; live qualification and catalog publication remain pending. Observations retain unknown effective privilege.
