# Providers 0.2.0 candidate

Status: exact packaged host acceptance passed; unpublished candidate.

## Exact revisions

- Provider source: [`fde7472c2c32a30bbd885f7c8db6d73b178f4aa1`](https://github.com/NIPE-Solutions/permesh-providers/commit/fde7472c2c32a30bbd885f7c8db6d73b178f4aa1)
- Permesh CLI `0.1.0-alpha.3` candidate source: [`add7a725745f5e05415b330e8d6be61fcaaf5d67`](https://github.com/NIPE-Solutions/permesh/commit/add7a725745f5e05415b330e8d6be61fcaaf5d67)
- [Native provider candidate run 34349822945](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34349822945): all five jobs succeeded
- [Provider CI 34348427195](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34348427195): Rust 1.94.1, Linux, macOS and Windows jobs succeeded
- [Dependency security 34348427173](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34348427173): succeeded
- [CLI attested candidate run 34351503021](https://github.com/NIPE-Solutions/permesh/actions/runs/34351503021): all five targets succeeded; its bundle and all 20 public API subjects passed independent verification

The native run built GitHub, Google, Cloudflare, AWS IAM, GitLab, Entra and AWS
Identity Center on Apple Silicon and Intel macOS, ARM64 and x86_64 Linux GNU, and
x86_64 Windows MSVC. Every job ran locked workspace tests, packaging tests, native
release builds, credential-free setup and negotiated-cancellation smoke checks,
and archive verification.

## Independent package verification

All 35 packages were downloaded from the same run and passed the existing package
verifier. The checks covered adjacent checksum sidecars, strict two-file ZIP
layout, catalog provider/version/target metadata, native executable headers, and
combined project/dependency notice bundles. No downloaded executable was run as
part of this independent archive verification.

The downloaded outer artifact ZIP bytes independently matched the upload digests
reported by GitHub:

| Target | Artifact ID | SHA-256 |
| --- | ---: | --- |
| Apple Silicon macOS | `10103618708` | `b80581f990dc658fd67a502c719bc9800a9179b344b9540a7c29a9b2e54a9e61` |
| Intel macOS | `10103995915` | `47283cb3e69cf325e9e770d30a4d7fc1e77efec6a5702ec74087518966759fb1` |
| ARM64 Linux GNU | `10103458506` | `277633eabfcd9641bbc01a9b090f1446046cb065fcbfdc092c3082e3521531d2` |
| x86_64 Linux GNU | `10103516732` | `3d331b6d5d5add3671cef497f6c96a82463e80fc75088d0bc5e229b133139097` |
| x86_64 Windows MSVC | `10103878768` | `17fb178a4c563b05817f4048f3c4723ff0d8383c502b20ea6b893ff06fe2bfe6` |

## Exact packaged host acceptance

A separately compiled harness using the alpha.3 source-matched `PackageStore`
validated and installed all 35 archives. This is package-format acceptance on
macOS, not execution of foreign-target binaries. All seven native macOS providers
passed draft 3 descriptions and negotiated handshakes. The exact packaged CLI,
SHA-256 `33d366c9b7481bed551b9de41a81af95324984f36d145a03288cf9b2147630a8`,
then exercised actual declarative setup, explicit trust, review and approval,
and exact five-target maps and native digest resolution. The maps were explicitly
applied to setup-generated local configuration, not selected through the public
catalog.

Negative checks confirmed that approval was not automatic. The host rejected a
wrong approval fingerprint, missing credentials, and missing or changed native
pins. These results qualify the exercised host and
package paths for the exact source and binary pair recorded here. They do not set a
minimum compatible host version or qualify different builds.

The exact GitHub provider binary, SHA-256
`9f2b82992c62767229af9af93dc5ddc3520d4e43e0a61513f621de1bb4688f02`,
also passed the separately authorized NIPE-Solutions doctor/details,
provider-status and stable-account JSON-query checks. Results retained the expected
visibility limitation and did not infer verified email or canonical identity.
Admin discovery was not repeated. No broader live provider or API-path acceptance
was performed.

## Remaining gates and limits

These are temporary, unsigned workflow artifacts. No release or catalog entry was
published. The evidence does not establish publisher authentication, signatures,
notarization, reproducible builds, complete license compliance, live API coverage,
other operating systems or protocol stability. Public 0.2.0 catalog-guided
installation and update remain open because no 0.2.0 entry or release exists; only
the published GitHub 0.1.0 catalog install/update path passed. Publication still
requires an explicit maintainer release operation, verification of the actual
release assets, and a separately reviewed catalog change.
