# Build a Permesh provider

A provider translates your system’s read-only access metadata into records Permesh
can validate and query. You can keep an internal provider in your own repository;
you do not need to change the CLI or join the official catalog.

## Choose the boundary

Use the shared Rust SDK and this repository’s native runtime for an official Rust
adapter. For another language, implement the public NDJSON subprocess protocol.
Production trust currently requires a self-contained native executable for each
supported target, not a shell command or interpreter plus script.

Start with the [protocol versioning rules](https://github.com/NIPE-Solutions/permesh/blob/main/docs/provider-protocol/versioning.md)
and [wire specification](https://github.com/NIPE-Solutions/permesh/blob/main/docs/provider-protocol.md).
Negotiated v1 is still a draft. Match the [pinned host contract](negotiated-v1.md);
do not treat legacy `protocol: 1` and negotiated `protocol_version: 1` as the same exchange.

## Build the existing adapters

Use Rust 1.94.1 or newer and Python 3.12 or newer:

```bash
git clone https://github.com/NIPE-Solutions/permesh-providers.git
cd permesh-providers
cargo build --workspace --locked
cargo test --workspace --locked
python3 scripts/smoke_provider.py --provider github target/debug/permesh-provider-github
```

On Windows use `python` if that is your Python command and append `.exe` to the
binary path. The smoke test runs known local code deliberately with synthetic
protocol input, no credentials, and no provider API requests. It does not grant
production trust or approve a workspace.

Read [GitHub](../providers/github) for access paths and
[Google](../providers/google) for directory identity assertions. Shared process
handling lives in [native-runtime](../crates/native-runtime); reusable HTTP
behavior lives in [network module](../crates/native-runtime/src/network.rs).

## Implement a useful slice

1. Define scope: accounts, resources, memberships, directory identities or access
   evidence. Document the API permissions and what remains invisible.
2. Implement health separately from discovery. A health check should diagnose
   connectivity and authentication without fetching the entire graph.
3. Normalize immutable IDs, preserve native roles and provenance, and distinguish
   observed assignments from effective authorization. Follow the
   [normalization guide](https://github.com/NIPE-Solutions/permesh/blob/main/docs/provider-development/normalization.md).
4. Declare setup fields and named credential slots. Use conditional steps for
   different authentication needs. See the
   [setup schema](https://github.com/NIPE-Solutions/permesh/blob/main/docs/provider-setup.md#provider-owned-schema-cli-owned-questions).
5. Handle pagination, rate limits, deadlines, cancellation and partial failures.
   Never translate permission denial into an apparently complete empty snapshot.

The host sends only configured credentials after validating the handshake and
workspace approval. Do not depend on inherited environment variables. Keep stdout
for protocol messages, never reflect credentials in errors, and use the explicit
[network context](network.md) when supported.

## Test before connecting live infrastructure

Run the checks in [CONTRIBUTING.md](../CONTRIBUTING.md). Test normalized snapshots
through the shared SDK contract validator and decode actual emitted protocol
records with the pinned host. Existing adapters demonstrate both layers.

Cover duplicate IDs, missing references, invalid parents, malformed API payloads,
pagination, authentication failures, throttling, partial completion and secret
redaction. Runtime tests cover process framing and cancellation; adapter tests
must also prove their own normalization and visibility semantics. Use fictional
identities and local mock servers, never private access reports.

## Try a local candidate

Build the matching current-source CLI first. Then follow its
[external trust workflow](https://github.com/NIPE-Solutions/permesh/blob/main/docs/external-providers.md)
to inspect the native binary, register its exact digest and capabilities, and
create a workspace instance. For a separately registered 0.2.0 candidate with a
setup description, explicitly select the discovery contract:

```bash
permesh provider setup github --id github-main --discovery-protocol negotiated-v1
```

This assumes you have explicitly trusted the local GitHub candidate under the
registration ID `github`. Substitute your own registered ID for another provider.
Review and approve the resulting instance before queries or credential delivery.
Never overwrite a trusted executable during development and expect its old trust
to remain valid; rebuild, inspect and register the new digest.

There is no production interpreter/developer-trust mode yet. The
[Python example](https://github.com/NIPE-Solutions/permesh/tree/main/examples/external-provider)
is an interoperability reference, not an installable plugin package.

## Share or publish

Internal providers can remain private and use local registration. To propose an
official adapter, submit source, synthetic tests and a provider guide first.
The guide must cover permissions, authentication, observed data, unprovable access,
rate limits, custom endpoint support, security and troubleshooting.

Only add catalog entries after [release qualification](releasing.md). Native
artifacts, checksums, compatibility metadata and documentation must describe the
same immutable release. Installation, binary trust and workspace approval remain
separate decisions even when a guided command presents them together.
