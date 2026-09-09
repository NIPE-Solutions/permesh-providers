# Permesh providers

**Connect your systems. Understand who has access.**

Official integrations for [Permesh](https://github.com/NIPE-Solutions/permesh),
the local-first CLI for answering **who has access to what—and why**.

Providers read access metadata from your services: accounts, directory identities,
teams, roles, and assignments. Permesh combines those observations into queries
such as `permesh user alice@example.com`, `permesh admins`, and `permesh orphaned`.

Install only the integrations you need. They run on your machine, use the
credentials you explicitly configure, and have no Permesh backend or telemetry.

[Available providers](#available-providers) · [Install](#connect-your-first-provider) ·
[Build your own](#connect-an-internal-or-unsupported-system) · [Contribute](CONTRIBUTING.md)

## Available providers

| Provider | What it discovers | Published package | Current source |
| --- | --- | --- | --- |
| [GitHub](docs/github.md) | Organization members/owners, repositories, teams, memberships and observed permissions | **0.1.0 — installable** | 0.2.0 candidate |
| [Google Workspace](docs/google.md) | Directory accounts and lifecycle; optional authoritative identities | Not published | 0.2.0 candidate |
| [Cloudflare](docs/cloudflare.md) | Account members, IAM groups, roles and scoped policy assignments | Not published | 0.2.0 candidate |
| [AWS IAM](docs/aws.md) | Users, roles, groups and managed/inline policy attachments | Not published | 0.2.0 candidate |
| [GitLab](docs/gitlab.md) | Explicit groups/projects, direct members and separately labeled collapsed effective membership | Not published | 0.2.0 candidate |
| [Microsoft Entra](docs/entra.md) | Proved-tenant directory accounts/identities, groups and direct user/group memberships | Not published | 0.2.0 candidate |
| [AWS Identity Center](docs/aws-identity-center.md) | Selected store accounts/groups, direct memberships and provisioned permission-set assignments for allowlisted accounts | Not published | 0.2.0 candidate |

**Installable** means qualified artifacts are listed in the [public catalog](catalog/v1.json).
**Candidate** means implemented source with offline tests; it is not an installable
release or a claim of complete live qualification. No provider is declared stable.

GitHub 0.1.0 is available for macOS Apple Silicon and Intel, Linux GNU ARM64 and
x86_64, and Windows x86_64. Recorded five-target runs cover earlier GitHub, Google,
Cloudflare and IAM source revisions. GitLab, Entra and Identity Center have local
synthetic/native checks; their five-target qualification remains pending. See the
[exact qualification matrix](docs/qualification.md). Candidates require a compatible current-source CLI; see
[candidate compatibility](docs/negotiated-v1.md). Source versions do not change
already published binaries.

Coverage matters more than the number of integrations. Google currently supplies
directory identities, not group or resource grants. AWS IAM inventories attachments;
the separate Identity Center binary covers provisioned assignment observations and
optional allowlisted organization account names. Neither evaluates effective AWS
permissions. Cloudflare assignments retain
unknown effective privilege. Each provider guide explains required permissions,
observed evidence, visibility limits, and authentication.

## Connect your first provider

[Install Permesh](https://github.com/NIPE-Solutions/permesh/blob/main/docs/installation.md),
then start with GitHub in a new directory:

```bash
mkdir company-access
cd company-access
permesh init --organization Acme
permesh provider add github
permesh auth login github-main
permesh doctor
permesh user YOUR_GITHUB_LOGIN
permesh admins
```

The guided add command selects a compatible published package, asks you to trust
its local execution, collects organization names and credential references, and
asks you to approve the resulting instance. Choose `keychain://github-main/token`
to use `auth login`. For an `env://` reference, supply the credential using your
existing secret tooling instead.

Start with a login; an email lookup needs verified identity evidence or an explicit
mapping to the immutable account ID. Read the [GitHub setup guide](docs/github-setup.md)
for permissions, multiple instances, scripting and updates.

**The unpublished providers cannot currently be installed from the catalog.**
Do not replace `github` with a candidate name and expect a public download.
Contributors can build and explicitly register candidates using the
[development guide](docs/development.md).

## Install and update deliberately

Use these commands to download a package separately from workspace setup:

```bash
permesh provider install github --version 0.1.0
permesh provider update github --check
permesh provider update github
```

`install` verifies and stores the package. It does not execute the provider or
approve credential delivery. `update --check` checks metadata; `update` downloads
a compatible newer version if available. Existing workspaces keep their selected
executable until you explicitly adopt and approve a change.

Normal queries never download or update providers. Artifacts and the catalog are
hosted on GitHub; there is no plugin service to sign into. Checksums bind bytes to
catalog entries, but the installer does not verify publisher signatures. Providers
run as your user and are not sandboxed. See [distribution and trust](docs/distribution.md).

## Connect an internal or unsupported system

You can write a provider without changing Permesh core or contributing it here.
The subprocess protocol is language-independent: send validated NDJSON over
stdin/stdout, and normalize the access metadata your system can expose.

Production registration currently accepts self-contained native executables.
A Python, Java or Node script is not directly installable as a command or interpreter
invocation. The [Python protocol example](https://github.com/NIPE-Solutions/permesh/tree/main/examples/external-provider)
is useful for learning the wire exchange; it does not bypass the native trust boundary.

[Build a provider](docs/development.md) covers the SDK, protocol examples,
normalization, setup forms, contract tests and explicit local registration.
Your provider can declare its own configuration fields, conditional setup steps,
and named credential slots. The CLI renders the prompts and handles references;
providers never need to implement their own terminal wizard.

Third-party providers use the explicit technical trust workflow. They are never
automatically executed because a cloned repository contains a plugin file.

## Contribute an official provider

A useful contribution starts with a clear answer: which access question can this
API answer, and what can it not prove?

1. Propose the scope and minimum read permissions in a [provider request](https://github.com/NIPE-Solutions/permesh-providers/issues/new).
2. Implement discovery with stable IDs, provenance and honest completeness.
3. Add synthetic API fixtures and shared contract tests, including failures.
4. Document authentication, setup, limitations and platform support.
5. Submit a PR using the [contribution guide](CONTRIBUTING.md).

Adding source does not automatically publish a package. Release qualification and
catalog review are separate steps. Improvements to existing API coverage, error
handling and documentation are just as welcome as new integrations.

## Documentation

| Need | Guide |
| --- | --- |
| GitHub setup, permissions and observations | [Setup](docs/github-setup.md) · [Coverage](docs/github.md) |
| An authoritative Google directory | [Google Workspace](docs/google.md) |
| Cloudflare membership and policy scope | [Cloudflare](docs/cloudflare.md) |
| AWS IAM attachments and credential requirements | [AWS IAM](docs/aws.md) |
| Scoped GitLab groups and projects | [GitLab](docs/gitlab.md) |
| Microsoft directory authority and direct memberships | [Entra](docs/entra.md) |
| AWS Identity Center assignment scope | [Identity Center](docs/aws-identity-center.md) |
| Corporate proxy or additional CA | [Networking](docs/network.md) |
| Develop and test an integration | [Provider development](docs/development.md) · [Contributing](CONTRIBUTING.md) |
| Understand candidates and upgrades | [Compatibility](docs/negotiated-v1.md) · [Lifecycle](docs/provider-lifecycle.md) |
| Publish and verify packages | [Release qualification](docs/releasing.md) · [Catalog format](docs/catalog.md) |
| Understand dependency choices | [Dependency policy](docs/dependencies.md) |

The [CLI, SDK and protocol](https://github.com/NIPE-Solutions/permesh) live in the
main repository. This repository owns official adapters, their tests, documentation,
and release catalog. Report vulnerabilities through [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Third-party dependencies retain their own licenses and required
notices in distribution archives. No paid provider packs or provider-count limits.
