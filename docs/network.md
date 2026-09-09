# Explicit enterprise network context

Current GitHub, Google and Cloudflare source candidates support an explicitly
approved HTTPS proxy and additional CA certificates. AWS does not support this
feature: it rejects `network_v1` before accepting a configured operation. The
published 0.1.x packages are unchanged; use a compatible host and a candidate
binary that supports this feature.

## Workspace configuration

The host owns network settings under an external instance, separately from the
provider's API configuration and credential references. For example, this field
excerpt adds a proxy, a bypass suffix, and a pinned workspace-relative CA bundle:

```yaml
external:
  discovery_protocol: negotiated_v1
  network:
    https_proxy: http://proxy.example:8080
    no_proxy: [.internal.example]
    ca_bundle:
      path: certificates/company-ca.pem
      sha256: REPLACE_WITH_EXACT_64_HEX_DIGEST
```

A proxy or CA bundle can be configured independently. A bypass list requires a
proxy. Review and approve the changed workspace before running an operation.
The host reads the bounded CA file and verifies its digest before resolving
provider credentials. Changes to approved network settings or pinned bytes
require explicit review; there is no automatic fallback to a direct connection.

Proxy URLs must use HTTP or HTTPS and cannot contain credentials, a query,
fragment, or a path other than `/`. Proxy authentication is unsupported. The
bypass list accepts at most 64 DNS names, DNS suffixes, IP addresses or `*`;
CIDRs, ports, URLs, whitespace and duplicate entries are rejected. CA files
must contain only PEM `CERTIFICATE` blocks, at most 64 certificates and 256 KiB.
Private keys, malformed certificates and mixed certificate/key bundles are
rejected. Paths must remain relative to the workspace without parent traversal.

## Negotiation and transport

The existing negotiated protocol version remains `1`. A host requesting network
context adds `features: [network_v1]` to its handshake. The provider must echo
that feature before the host sends credentials and the `network` context in the
configured `check` or `discover` invocation. Missing, unsupported, duplicate or
unsolicited features fail closed. The runtime requires context presence to match
the negotiated feature exactly. Existing handshakes without features retain
their original response bytes and record capabilities.

The shared runtime offers an explicit `serve_with_network` factory API and an
adapter opt-in. The original `serve` API rejects network requests even if an
adapter claims support, so a factory cannot silently ignore approved settings.
All CA certificates are parsed before the runtime calls a credential-consuming
factory.

GitHub and Cloudflare apply the context to all API requests. Google applies it
to Directory requests and OAuth refresh-token exchanges. API endpoints and
permission scopes are unchanged. HTTPS requests use the explicit proxy, subject
to the explicit bypass list. These clients disable ambient proxy discovery.
Additional CA roots augment platform trust; hostname and certificate validation
remain enabled. The host sanitizes the child environment, including proxy and
CA environment overrides. Standalone binaries retain platform trust behavior.

The host's interactive browser login does not yet support this context. Use an
approved access-token or refresh-token configuration when explicit network
settings are required. Setup and browser-auth descriptions remain credential-free
legacy operations and do not negotiate network settings.

## Offline verification

Synthetic tests exercise successful HTTPS through a local CONNECT proxy, bypass
to the same local TLS server, CA-required success, rejection without the CA, and
hostname mismatch rejection. Subprocess tests verify hostile ambient proxy
settings cannot override explicit routing, all three provider API clients use
the context, Google's refresh exchange uses it, and AWS rejects the feature.
No live service or production credential is used by these tests. Live enterprise
proxy and certificate deployment qualification remains separate.
