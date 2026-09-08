# Catalog schema 1

The public entry point is `catalog/v1.json` on `main`. It currently contains no
releases. An empty catalog is valid and intentionally advertises no installable
provider. Add a release only after native artifact qualification.

The top-level object has exactly `schema_version: 1` and `releases`, an array of
release objects. Each object has exactly these fields:

| Field | Contract |
| --- | --- |
| `provider` | Lowercase ASCII letter followed by lowercase letters, digits, hyphens or underscores; at most 32 bytes |
| `version` | Exact stable semantic version, without prerelease or build metadata |
| `target` | One supported target from the list below |
| `capabilities` | Unique entries from accounts, identities, resources, groups, memberships, grants |
| `protocols` | Unique entries from 2 and 3; draft 2 required |
| `archive_sha256` | Lowercase 64-character SHA-256 hex digest of the ZIP asset |
| `executable_sha256` | Lowercase 64-character SHA-256 hex digest of the executable |
| `archive_size` | Exact positive compressed byte count, at most 128 MiB |

The `(provider, version, target)` coordinate is unique. Unknown and duplicate
fields are invalid. Catalogs are limited to 1 MiB and 4,096 release entries. Keep
entries ordered by provider, numeric version, then target for reviewable diffs.
Never change digests, capabilities or other metadata for an existing coordinate;
publish a new version for corrections.

Supported targets:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `aarch64-unknown-linux-gnu`
- `x86_64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`

## Assets

Create tag `PROVIDER-vVERSION` in this repository and attach
`permesh-provider-PROVIDER-VERSION-TARGET.zip`. The client derives the URL; catalog
entries do not contain arbitrary URLs, scripts or install hooks.

Each ZIP has exactly two regular files at its root: `provider` (or `provider.exe`
on Windows) and `LICENSE`. The executable is at most 128 MiB and the license at
most 1 MiB. Include all notices required by the executable's dependencies in the
license file. Use stored or DEFLATE compression. No directories, links, duplicate
entries, extra files, encryption or alternative compression methods are accepted.
ZIP64, extra fields, archive comments, prefixed data and trailing records are
rejected by this deliberately narrow package profile.
The executable header must match the declared platform and architecture.

## Publication gate

Build and test from a reviewed source commit, qualify all declared target assets,
verify archive/executable checksums and sizes, then submit the catalog entry as a
pull request. Include source revision, test evidence, permissions, limitations
and release notes. Verify the actual uploaded assets before merging the entry.
There is no automated publisher in this repository yet.

Checksums are compared against the fetched catalog; the initial client does not
verify independent signatures or attestations. Repository ownership and release
permissions are part of the trust boundary. Installation does not execute or
trust binaries, and does not change workspace pins. See
[distribution.md](distribution.md) for update and rollback behavior.
