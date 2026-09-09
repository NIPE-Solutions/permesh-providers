# Providers 0.2.0 evaluation prereleases

Status: original qualified packages published as evaluation prereleases on
2026-09-09. Each of the seven releases contains five ZIPs and five checksum
sidecars. All 70 assets were verified after staging and again through
unauthenticated public downloads; all 35 downloaded ZIPs passed the existing
package verifier against original catalog metadata. Tags resolve to the exact
provider source below. Previous releases and drafts remain unchanged.

Upgrade to Permesh CLI **0.1.0-alpha.3** before using this catalog. Alpha.2 rejects the new negotiated-v1 metadata even when selecting legacy 0.1.0. Existing installed packages, trust and workspace pins are not changed by a catalog update.

Public catalog installation acceptance passed for the scope recorded below.

## Published assets

[github 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/github-v0.2.0), [google 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/google-v0.2.0), [cloudflare 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/cloudflare-v0.2.0), [aws 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/aws-v0.2.0), [gitlab 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/gitlab-v0.2.0), [entra 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/entra-v0.2.0), [aws-identity-center 0.2.0](https://github.com/NIPE-Solutions/permesh-providers/releases/tag/aws-identity-center-v0.2.0).

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

## Public catalog acceptance

The separately reviewed [catalog PR 23](https://github.com/NIPE-Solutions/permesh-providers/pull/23)
merged as `5ecb6d6be86b2e50cd01d6e81cd074735b298427`, adding the 35 original
0.2.0 entries while preserving all five legacy entries. On 2026-09-09, the
publicly downloaded alpha.3 Apple Silicon CLI installed all seven native 0.2.0
packages through the production catalog and passed idempotent exact updates.
Every executable digest matched the original metadata.

GitHub, Google, Cloudflare and AWS IAM passed guided portable setup, including
rejection without explicit consent and correct five-target catalog pins. GitLab,
Entra and Identity Center passed public installation followed by explicit native
trust, negotiated setup, review and approval; their reviewed five-target maps were
applied explicitly. Alpha.3 does not offer guided add for those three types.
Wrong approval fingerprints were rejected on the explicit path. All seven rejected
absent credentials before API access. No live-provider credentials were supplied.

An existing GitHub 0.1.0 workspace under alpha.3 retained its selected pin, exact
workspace bytes, fingerprint and approval after a public update check found 0.2.0
and an explicit update downloaded it. Downloading did not adopt a new workspace
pin. This does not qualify migration of alpha.2 approvals. Temporary acceptance
state was removed; no new live-provider acceptance is claimed.

## Remaining gates and limits

These are unsigned evaluation releases promoted from the original workflow artifacts. The evidence does not establish publisher authentication, signatures,
notarization, reproducible builds, complete license compliance, live API coverage,
other operating systems or protocol stability. Public catalog acceptance is recorded above; broader live and desktop acceptance
remain separate gates.
