# Provider distribution contract

Status: accepted direction for the next implementation milestone. Catalog and
package schemas, installation and update commands are not implemented yet.
These requirements guide their implementation and acceptance tests.

## Ownership and catalog

`NIPE-Solutions/permesh-providers` owns official provider implementations, release
assets and a Git-reviewed catalog. The catalog is a static directory of releases,
not a service, executable manifest or remote configuration channel. Each provider
has its own semantic version and release tag; updating one does not update others.

A versioned catalog must identify the provider, exact version, supported target,
protocol compatibility, capabilities, asset size, archive digest and executable
digest. All fields are strictly validated. Catalog entries name immutable release
artifacts, never a floating executable URL. A changed digest for an already known
version is an error. Release publication precedes catalog inclusion so users
cannot discover incomplete releases.

GitHub distributes catalog metadata and packages. Explicit install/update commands
may contact GitHub and its documented asset-delivery hosts. They send no workspace
configuration, identities, reports or provider credentials. Ordinary queries,
setup, doctor and authentication do not fetch catalogs or check for updates.
There is no project-operated backend, scheduled updater or telemetry endpoint.

## Planned command behavior

| Command | Effect |
| --- | --- |
| `permesh provider install github --version VERSION` | Fetch and verify one exact compatible release for this platform. |
| `permesh provider update github --check` | Compare installed and available versions without installing, executing or changing workspace state. |
| `permesh provider update github` | Install the newest compatible stable release; never automatically downgrade or select a prerelease. |
| `permesh provider update github --version VERSION` | Select an exact compatible release, including an intentional rollback. |

The first implementation updates one named provider. No implicit bulk update,
background download or update check is part of this contract. Machine output must
report installed and selected versions, digests, compatibility and whether local
installation changed, using Permesh's versioned output envelope. Noninteractive
operation must be explicit and must not bypass execution approval.

`--check` needs no acceptance of executable code because it downloads metadata
only. Installing a package must show its source, version and verified digest.
Installation stores bytes; execution remains subject to explicit local trust.
Setup, validation hooks and provider binaries are never run by the installer.

## Versioned local storage and adoption

Store verified versions side by side under platform-appropriate local application
data directories. This requires extending the current single-registration model
before implementing updates. A new download must not overwrite an executable
used by an existing workspace or remove its locally approved version.

An update can select a new version for future setup, but must leave existing
workspace pins, configuration and execution approvals unchanged. Adopting that
version in a workspace requires an explicit, reviewable pin change and approval
for the new digest and configuration. Provider capabilities cannot silently expand
an existing grant of trust. Configuration migrations must produce a reviewable
change; query commands never perform them.

Retain the previous installed version for rollback. Do not implement automatic
garbage collection until references and retention rules are defined. Rollback
selects an existing verified version or downloads an exact release; it does not
restore trust by bypassing the normal approval checks.

## Download and extraction boundary

Use bounded metadata, download sizes, extraction sizes and timeouts. Validate
HTTPS destinations and redirects against the supported distribution origins.
Verify archive and executable digests before registration. Checksums detect
corruption and mismatches; a checksum hosted beside a compromised package is not
independent proof of publisher authenticity. GitHub repository and release
permissions are part of the trust boundary. Evaluate artifact attestations during
release implementation and document exactly what the client verifies.

Extract only an explicitly enumerated package layout. Reject absolute paths,
parent traversal, symlinks, hardlinks, duplicate entries, device files and extra
executables. Account for Windows path separators, reserved names and case
collisions. Use private temporary storage and atomic final placement. Failed or
cancelled downloads, malformed archives and digest mismatches leave existing
installations and workspace files intact. Concurrent installers must not replace
or partially publish each other's versions.

Archives must contain the native provider executable and required license notices,
with documented metadata if the package schema requires it. No post-install
scripts, shell commands, dependency installers or self-updaters are allowed.

## Acceptance before enabling commands

Use synthetic local fixtures for catalog parsing, version selection, compatibility,
redirect policy, bounded downloads, hostile archives, concurrent installation,
cancellation and rollback. Verify no package execution or credential resolution
occurs. Test both human and JSON output, exit codes, offline errors and unchanged
workspace pins. Run native tests on macOS, Linux and Windows. Document limitations
before listing a provider as installable.
