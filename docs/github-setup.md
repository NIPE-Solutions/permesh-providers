# Configure the GitHub provider

Use the qualified `0.1.0` package from the official catalog. Review the release
and its [coverage limitations](github.md) before granting execution trust.

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
identity alias mapping the account's immutable numeric GitHub ID (as a string)
to the canonical identity, or look up the known GitHub login directly. Approval binds the full normalized workspace;
configuration edits, including aliases, require another review and approval.

Health tests authentication and active organization membership. Successful health
cannot prove every discovery endpoint is visible. Access results preserve partial
failures and the permanent visibility limitations described in the provider docs.

## Existing workspaces and updates

After installing and explicitly trusting the reviewed GitHub binary, convert an
existing built-in instance while preserving its ID and credential reference:

```bash
permesh provider migrate github-main --sha256 REVIEWED_SHA256
```

Review the Git diff and run the review/approve steps above. Migration does not
execute code, resolve credentials or grant approval. It changes the full workspace
fingerprint, so every affected external instance needs renewed review/approval.
Do not create a duplicate instance with another ID just to migrate; aliases and
provider account identifiers rely on the existing instance ID. See the
[CLI migration guide](https://github.com/NIPE-Solutions/permesh/blob/main/docs/github-migration.md).

`permesh provider update github --check` is metadata-only. `provider update github`
downloads a newer stable package, but existing workspaces keep their old trusted
pin. Trust the reviewed new bytes, explicitly edit the workspace pin, and review
and approve the changed configuration to adopt an update. An earlier retained pin
can be restored and approved to roll back. Query commands never update providers.
