# Supplemental distribution notices

Some registry archives omit their upstream license file. Packaging still requires
license text; an SPDX identifier alone is insufficient. `notice-supplements.json`
records narrowly reviewed exceptions, not a fallback license template.

`base64-simd 0.8.0` and `vsimd 0.8.0` both identify upstream revision
`d74c030d9dc4f3cae02146d1f497ff62726ef09a` in their published
`.cargo_vcs_info.json`. Their manifest SPDX expression is MIT. The included
[MIT text](licenses/nugine-simd-MIT.txt) is copied byte-for-byte from
[that revision's LICENSE](https://github.com/Nugine/simd/blob/d74c030d9dc4f3cae02146d1f497ff62726ef09a/LICENSE).
Its SHA-256 is
`71674605ec4c087fe9eb534e3e4f9e26eb2e4aabcd76a29fd156c6a844d44b3d`.

The offline packager accepts a supplement only when package name, version,
crates.io source, SPDX expression, repository, locked registry archive checksum,
upstream revision/path and local notice digest match the mapping. Cargo's locked
build validates downloaded registry archives against those checksums. Notice
paths must remain under `third-party/licenses`; existing regular-file and size
checks apply. The license bundle preserves upstream copyright and permission
text. New versions or sources require a fresh review when their archives still
omit notices. Packaging never downloads a missing notice and never edits the
Cargo cache.
