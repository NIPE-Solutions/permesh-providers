# Configure the GitHub provider

The package is being qualified. These instructions become usable once version
`0.1.0` is listed in the catalog; do not substitute an unreviewed executable.

## Install and trust

```bash
permesh provider install github --version 0.1.0
```

Review the release, capabilities and checksum. The command reports the installed
executable path and SHA-256. Substitute those exact values below:

```bash
permesh provider external trust /absolute/path/to/provider \
  --id github --sha256 REVIEWED_SHA256 \
  --capability accounts --capability resources --capability groups \
  --capability memberships --capability grants --accept-risk
```

On Windows the executable is `provider.exe`; quote paths containing spaces.
Native providers run with your user permissions and are not sandboxed. Trust
copies the inspected bytes into private local registration storage. It does not
start discovery or resolve a credential.

## Create an instance

From a Permesh workspace, run:

```bash
permesh provider setup github --id github-main
```

The CLI asks for organization names and a token **reference**, such as
`env://PERMESH_GITHUB_TOKEN` or `keychain://github-main/token`. Do not enter an
actual token into setup. For scripting, use `--answers answers.yaml`:

```yaml
version: 1
answers:
  organizations:
    - example-org
  token: env://PERMESH_GITHUB_TOKEN
```

Setup executes the trusted binary to obtain its form, then writes a digest-pinned
external instance. Review the Git diff. It does not resolve or store credentials.
Use `--describe` to inspect the form without changing the workspace.

Provide the environment variable using your existing secret tooling. For a
keychain reference, use:

```bash
permesh auth login github-main --credential token
```

See [permissions and coverage](github.md) before creating the token.

## Review and query

```bash
permesh provider external review github-main
permesh provider external approve github-main --fingerprint REVIEWED_FINGERPRINT --accept-risk
permesh doctor
permesh user alice@example.com
```

GitHub public email is not verified identity evidence. Configure an explicit
identity alias mapping the account's GitHub login to the canonical identity, or
look up the known provider account. Approval binds the full normalized workspace;
configuration edits, including aliases, require another review and approval.

Health tests authentication and active organization membership. Successful health
cannot prove every discovery endpoint is visible. Access results preserve partial
failures and the permanent visibility limitations described in the provider docs.

## Existing workspaces and updates

Keep a working built-in `type: github` instance until this external package passes
qualification. Do not add a second copy to the same workspace merely to migrate:
that would represent the same accounts under different provider instance IDs.
The migration must preserve the existing instance ID, aliases, organizations and
credential reference while introducing an explicitly reviewed executable pin.

`permesh provider update github --check` is metadata-only. `provider update github`
downloads a newer stable package, but existing workspaces keep their old trusted
pin. Trust the reviewed new bytes, explicitly edit the workspace pin, and review
and approve the changed configuration to adopt an update. An earlier retained pin
can be restored and approved to roll back. Query commands never update providers.
