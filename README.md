# Permesh providers

Official providers for [Permesh](https://github.com/NIPE-Solutions/permesh).

**Know who has access to what.**

This repository is the home for independently distributed, read-only Permesh
providers and their release catalog. Providers discover access metadata directly
from the services users configure. Permesh has no backend or telemetry.

## Status

The [GitHub provider](docs/github.md) has a native subprocess implementation under
`providers/github`, with the original API contract tests and draft 2/3 integration.
The versioned [catalog](catalog/v1.json) remains empty until all five native
release candidates pass qualification.
Existing built-in providers continue to work during that transition.

Permesh already supports explicitly trusted external binaries, workspace approval,
credential delivery and declarative setup. Catalog installation and explicit updates are implemented on the CLI development
main branch. No downloadable provider release is available yet:

```bash
permesh provider install github --version VERSION
permesh provider update github --check
permesh provider update github
permesh provider update github --version VERSION
```

Updates are requested by the user. Ordinary access queries never check for
updates. Downloading a package does not execute it, resolve credentials, or
approve a workspace. Version selection and workspace adoption are separate
operations; see the [distribution contract](docs/distribution.md).

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

- [GitHub permissions and coverage](docs/github.md)
- [GitHub installation and setup](docs/github-setup.md)
- [Dependency policy](docs/dependencies.md)
- [Release qualification](docs/releasing.md)
- [Catalog schema and archive layout](docs/catalog.md)
- [Distribution and explicit updates](docs/distribution.md)
- [Provider migration and release acceptance](docs/provider-lifecycle.md)
- [Contributing](CONTRIBUTING.md)
- [Security reporting](SECURITY.md)

## License

MIT OR Apache-2.0, at your option. Both license texts are in [LICENSE](LICENSE).
