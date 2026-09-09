# Dependencies and source provenance

The first GitHub provider reuses the tested Rust adapter from Permesh revision
`0fd9ac4f059cbd2f2f7a2d16854c758dcd66fc0c`. Current core, SDK, protocol and secret
crates are pinned to `0196197a215c0b244251fe1e3ca1eb759159776b`, which provides
independent negotiated v1 wire DTOs, richer domain semantics and optional network-context negotiation. The 0.2.0
adoption changes these four Git pins and workspace package versions without
updating registry dependencies. Exact revisions remain required until published
SDK releases provide a stable boundary. No sibling checkout is needed to build this repository.
Review SDK changes as protocol compatibility changes and commit the lockfile.

The adapter retains its existing dependency choices: Tokio for bounded async I/O
and cancellation, reqwest with explicit Rustls/JSON/query features for GET-only
HTTP, Serde for strict typed JSON, time for UTC timestamps and HTTP retry dates,
and zeroize for secret-bearing request buffers. The executable uses the standard
library for process entry and accepts no command-line credentials. It does not
need a separate CLI parser or terminal styling library.

The crates are inherited from the already qualified Permesh dependency graph;
see its [dependency review](https://github.com/NIPE-Solutions/permesh/blob/0fd9ac4f059cbd2f2f7a2d16854c758dcd66fc0c/docs/DEPENDENCIES.md)
for upstream maintenance, license and feature decisions. New dependencies still
require a concrete need and review. Lockfile updates must pass native tests,
`cargo-deny` and `cargo-audit` before merge.

`deny.toml` permits crates.io and only the exact Permesh Git repository, requiring
revision-based Git dependencies. It does not trust every repository in the
organization. This uses cargo-deny's documented
[source policy](https://embarkstudios.github.io/cargo-deny/checks/sources/cfg.html).
Duplicate dependency versions are reported for review; unknown sources and
unapproved licenses fail CI. No advisory is silently ignored.

Packaging uses Python's standard library and fixed ZIP metadata. Each archive
contains a combined license file for the project and locked reachable dependency
sources, including required notices. Review that file before release. These
notices do not change a dependency's license or imply that all dependencies use
the project license. Checksums provide integrity; signature and attestation
verification are not yet implemented by the installer.

The network-context runtime shares the existing reqwest dependency to configure
explicit proxies, bypass rules and additional CA roots consistently. Test-only
rustls and tokio-rustls dependencies use versions already present in the lockfile
to serve synthetic localhost HTTPS fixtures. They add no production transport or
certificate-verification bypass. The fixture server key is public test data.
