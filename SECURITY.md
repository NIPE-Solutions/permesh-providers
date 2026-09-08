# Security policy

No provider releases have been published from this repository yet.

Report vulnerabilities privately through
[GitHub security advisories](https://github.com/NIPE-Solutions/permesh-providers/security/advisories/new).
Do not include credentials, private infrastructure data or exploit details in
public issues. Include the affected revision, platform, a synthetic reproduction
and the expected security boundary. No response-time SLA is established.

Provider executables run as the invoking user and are not sandboxed. Explicit
local trust and workspace approval remain required. Packages must not include
post-install execution. See the [distribution trust boundaries](docs/distribution.md).
Report protocol or CLI vulnerabilities to the
[Permesh repository](https://github.com/NIPE-Solutions/permesh/security).

If a credential is exposed, revoke it with its issuer; removing a log or file does
not invalidate the credential. Maintainers should reproduce with synthetic data,
coordinate disclosure, add regression coverage and publish corrected versions.
