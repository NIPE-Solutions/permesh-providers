# Qualify and release a provider

GitHub provider releases use the tag `github-vVERSION` and five native targets:
macOS ARM64 and Intel, Linux GNU ARM64 and x86_64, and Windows x86_64 MSVC.
The first version is `0.1.0`. This is separate from the CLI's release version.

## Qualification

Open a pull request with source, tests, lockfile, permission/limitation docs and
release notes ready for review. CI runs formatting, strict Clippy, tests on Linux,
macOS and Windows, the declared Rust 1.91 minimum, and dependency checks. Native
candidate jobs test and build on each target's native runner with locked Cargo
dependencies; they do not cross-compile or use live provider credentials.

Each native job also exercises the real executable's draft 3 setup protocol with
an empty environment and temporary working directory. Packaging uses fixed ZIP
metadata and includes exactly `provider` (`provider.exe` on Windows) and `LICENSE`.
The combined license includes source-supplied dependency licenses and notices from
the locked, target-filtered Cargo graph. Missing, linked or oversized inputs fail
qualification. A separate verifier checks the archive profile, target, checksum,
executable digest and catalog metadata.

Download the artifacts from the successful candidate run for the exact reviewed
PR head. Each target artifact contains its ZIP, adjacent `.sha256`, and
`catalog-entry.json`. Independently verify every archive and install it through
Permesh's actual package validator in isolated local storage. Test the local native
executable through install/trust/setup/review/approval, preserving the separation
between those actions. Use synthetic HTTP fixtures for ordinary protocol tests.
If performing a credentialed acceptance exercise, keep credentials and reports
outside Git, Actions logs and release assets; record only aggregate results.

Merge only after all required jobs and review pass. Confirm that the merged tree
matches the qualified tree. Keep the exact source revision, workflow run and
qualification results in the release notes. Rebuilding a changed tree requires
new qualification.

## Publication

Publication is an explicit maintainer operation; PR workflows have `contents:
read`, no publication credential, and pinned third-party actions. Candidate
artifacts expire after seven days. Create a draft release targeting the qualified
merge commit, upload all five ZIPs and checksum sidecars, then verify asset count,
names, sizes and digests before publishing. Never overwrite an existing version.
A correction requires another version and qualification.

After publication, download the actual GitHub release assets and verify them
again. Add the five matching catalog entries through a separate reviewed pull
request only when all assets exist and pass validation. The directory stays empty
until the first release is qualified. Ordinary access queries do not contact it.

## Local checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo build --release --locked --target aarch64-apple-darwin
python3 scripts/smoke_provider.py target/aarch64-apple-darwin/release/permesh-provider-github
python3 scripts/package_provider.py --target aarch64-apple-darwin --output candidate-output
python3 scripts/verify_package.py candidate-output/permesh-provider-github-0.1.0-aarch64-apple-darwin.zip --entry candidate-output/catalog-entry.json
```

Substitute the local native target. Packaging requires a new output directory.
Identical inputs produce identical ZIP bytes with the same Python/compression
implementation; this does not claim reproducible Rust builds. Packages are
unsigned and not notarized. Checksums establish integrity, not independent
publisher authenticity. The installer does not yet verify signatures or artifact
attestations; GitHub repository/release control remains a distribution trust
boundary. An SBOM is not currently shipped. Do not describe those features as
implemented until their generation and client verification are tested.
