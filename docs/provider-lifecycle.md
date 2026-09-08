# Provider lifecycle

## First migration: GitHub

The existing GitHub adapter remains available in the Permesh CLI while an external
package is qualified. Move its implementation and tests with source provenance
preserved; do not replace a working adapter with a new untested implementation.
The CLI keeps the domain model, provider SDK, protocol, credential resolution and
presentation. Providers normalize API responses and preserve access provenance;
they do not own identity correlation or terminal formatting.

The external implementation and original API tests live under `providers/github/`.
Use exact SDK/protocol revisions until published versions provide a dependable
compatibility boundary. Do not add empty provider crates or catalog entries for
providers without usable releases.

Qualification requires:

1. Read-only GitHub behavior matching the existing adapter, including pagination,
   bounded retries, rate limits, actionable errors and inheritance paths.
2. The same synthetic provider contract and HTTP tests, plus equivalence checks
   comparing normalized results through built-in and subprocess paths.
3. Named credential delivery after handshake, cancellation cleanup, bounded
   protocol messages and no secret-bearing diagnostics.
4. A declarative setup form for organization settings and credential references;
   every generated configuration must pass workspace validation.
5. Native builds and offline smoke tests for macOS ARM64 and x86_64, Linux ARM64
   and x86_64, and Windows x86_64.
6. Permission documentation, observable-access limitations, authentication and
   revocation instructions, and a migration guide for existing workspace pins.

Only remove the bundled adapter after users have a working, tested installation
and migration route. Apply the same gate to later provider migrations.

## Releases

Release each provider independently. Use provider-specific tags such as
`github-vVERSION`; document the concrete version and asset schema before the
first release. Build from reviewed commits with least-privilege workflows and
pinned build dependencies. Ordinary pull requests must not receive publication
credentials. Publication is an explicit maintainer operation after qualification; candidate
workflows have no publication privileges. See [releasing](releasing.md).

Publish native archives, checksums, release notes and applicable license notices.
Qualify downloaded archives as well as build outputs. Add the release to the
catalog through a reviewed pull request only after all required assets exist and
pass verification. Never replace bytes of an existing version; publish a new
version for corrections. Record compatibility and known limitations in the release
notes. Release signing/attestation and SBOM tooling will be selected and tested
with the release implementation, rather than claimed before it exists.

## Additional providers

Providers may use any language that can implement the subprocess protocol, but an
official downloadable package must have a documented runtime and packaging model.
The initial installer targets self-contained native executables. Interpreted
provider installation is not implied by protocol language independence.

A new provider needs a clear access-metadata use case, read-only permission model,
synthetic contract and API tests, documented incomplete paths, credential handling
and a qualified release. Provider count is never a license or commercial gate.
