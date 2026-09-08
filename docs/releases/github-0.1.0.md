# GitHub provider 0.1.0 qualification

Qualified on 2026-09-08 from source commit
`5a4915a3eeaef926cbb972a22edc6d41dfac089a`, whose tree matches reviewed PR head
`1c8a967f38b42aa199f599fee01fc77a9cf4920b`.

- [Provider PR 4](https://github.com/NIPE-Solutions/permesh-providers/pull/4): all 15 checks passed.
- [Native candidate run](https://github.com/NIPE-Solutions/permesh-providers/actions/runs/34247700899): five native targets passed. The Intel macOS artifact-service finalization failed once after successful tests/build/package verification; the unchanged job passed on retry.
- 26 unit/API/protocol tests and five actual subprocess tests passed. Tests cover pagination, partial observations, permissions, bounded retries, cancellation, strict input, secret omission, setup and held-open stdin shutdown.
- Formatting, strict Clippy, Rust 1.91 and dependency security checks passed.
- Six packaging tests passed. Each ZIP contains exactly the executable and combined project/dependency license notices, with fixed archive metadata and adjacent SHA-256.
- Every downloaded native artifact passed the actual Permesh package store validator, including target headers, sizes, extraction rules and both digests. Uploaded release assets matched the qualified bytes.
- The Apple Silicon release executable passed install validation, reviewed binary trust, declarative setup, explicit migration and workspace approval. A read-only credentialed check confirmed health, canonical user-query and privileged-access parity with the original bundled adapter. Only aggregate pass/fail results were retained; no credentials, identities, organization details or access reports are included here.

The native environments were macOS 15 (ARM64 and Intel), Ubuntu 24.04 (ARM64 and
x86_64 GNU), and Windows Server 2025 (x86_64 MSVC). This does not establish support
for every older operating-system version. API observations are not transactional;
[visibility limitations](../github.md) remain part of every result.

Packages are unsigned and not notarized. Installer verification is SHA-256-based;
signature/attestation verification and SBOM delivery are not implemented. This is
a provider release, separate from the CLI's public release qualification.
