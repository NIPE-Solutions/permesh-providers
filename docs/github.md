# GitHub provider

The official adapter supports GitHub.com organizations. It sends only GET requests to `https://api.github.com`, using REST API version `2026-03-10`. Enterprise/custom endpoints are not configurable. Inspection never changes permissions or invitations.

## Credentials and health

Configure organization names and a secret reference using the workspace schema. Resolve tokens through environment variables or the OS credential store; do not put tokens in workspace YAML. The external provider accepts `organizations` as a list of organization names and a named `token` credential delivered by Permesh after the protocol handshake. It rejects custom API endpoints and unknown settings. See [installation and configuration](github-setup.md).

Health performs `GET /user`, then `GET /user/memberships/orgs/{org}` for each configured organization, requiring active membership. It does not enumerate the graph. This flow supports personal access tokens and GitHub App user tokens; installation tokens cannot satisfy `/user`. Successful health does not establish all discovery permissions.

For classic PATs, use `read:org` and `repo` for the full supported scope. Fine-grained tokens need organization **Members: read**, repository **Metadata: read**, and the relevant repositories selected. Organization approval and SSO authorization may apply. Collaborator enumeration additionally requires the authenticated user to have write, maintain, or admin access. Consult GitHub's endpoint requirements for [organization membership](https://docs.github.com/en/rest/orgs/members), [teams](https://docs.github.com/en/rest/teams/teams), and [collaborators](https://docs.github.com/en/rest/collaborators/collaborators).

## Observations

| Data | REST endpoints | Meaning |
| --- | --- | --- |
| Organization | `/orgs/{org}` | Resource and membership group |
| Members/owners | `/orgs/{org}/members?role=member` and `?role=admin` | Native organization role; `admin` normalizes to owner privilege |
| Repositories | `/orgs/{org}/repos` | Repository resources |
| Teams | `/orgs/{org}/teams` | Team groups |
| Team membership | `/orgs/{org}/teams/{slug}/members?role=all` | Membership observation, potentially inherited |
| Team repository access | `/orgs/{org}/teams/{slug}/repos`, then `/orgs/{org}/teams/{slug}/repos/{owner}/{repo}` | Repository media type obtains the team's effective role |
| Collaborator access | `/repos/{owner}/{repo}/collaborators?affiliation=all` | Effective access, never labeled direct |

Account keys use raw stable numeric GitHub IDs, qualified by provider ID. Resource and group IDs use `organization:`, `repository:`, and `team:` prefixes. Renames do not change identity. Public email is omitted because it is not established as verified. Core performs identity correlation.

Native role names are retained. Repository read/pull/triage normalize to standard, write/push/maintain to elevated, and admin to admin. Custom or missing names normalize to unknown. Permission booleans do not establish custom-role semantics. Failed team permission lookups retain the relationship with an unknown role and a partial warning.

[Collaborator roles](https://docs.github.com/en/rest/collaborators/collaborators) combine repository, team, organization, and enterprise sources. [Team permission checks](https://docs.github.com/en/rest/teams/teams#check-team-permissions-for-a-repository) include parent-team inheritance. Provenance method names contain `effective` for these observations. Organization ownership is not expanded into assumed repository grants.

## Completeness and bounds

All snapshots include permanent visibility limitations. `complete=true` means planned bounded collection finished; it does not prove all access was visible. Token permissions, selected repositories, SSO, and secret teams can restrict results. Pending invitations, enterprise policy, organization base-permission settings, non-repository resources, team maintainer roles, and explicit nested-team hierarchy are not enumerated. Collection is not transactional.

Failed endpoints, malformed records, invalid pagination, and exhausted budgets preserve useful observations with `complete=false` and sanitized category warnings. If no organization can be observed, discovery returns an error. Final snapshots pass the shared SDK validator.

Limits per discovery:

- 2 MiB per response, including streamed bodies without a content length.
- 100 rows per page and 100 pages per list.
- 20,000 source rows and 2,000 HTTP attempts globally. Normalization produces at most four graph records per source row, plus bounded organization records.
- 15 seconds per request including body reads; 5 seconds to connect.
- Two retries at most for rate-limit 403/429 and 5xx GET responses.

Retries honor `Retry-After` seconds/HTTP dates and primary-limit reset timestamps, with bounded jitter up to 250 ms. Secondary-limit responses without retry headers wait at least 60 seconds. Server delays over 60 seconds return a rate-limit failure instead of retrying prematurely. The caller supplies an overall deadline; cancellation drops requests and retry waits without detached jobs. See [GitHub rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api).

Next links are validated against the same origin, path, filters, and sequential numeric page. Credentials/fragments are rejected. The next URL is constructed locally. A full page without Link triggers a bounded next-page probe; Link without a next relation terminates even a full final page. Redirects are disabled. See [GitHub pagination](https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api).

Errors omit raw responses, URLs, headers, tokens, and transport diagnostics. A bounded 403 body is inspected only for documented rate-limit categories. A private test helper permits loopback HTTP; no endpoint override is public or available through workspace configuration.

## Tests

Run `cargo test -p permesh-provider-github`. Mock HTTP tests cover normalized snapshots, owners/custom roles, effective access, email exclusion, empty/partial results, pagination, redirects, malformed JSON, authentication/rate-limit/server failures, retry dates, resource budgets, health scope, timeout, and cancellation. Snapshot tests use the same validator as production.

## Source provenance and protocol

The API adapter and its HTTP contract tests were moved from Permesh revision
`0fd9ac4f059cbd2f2f7a2d16854c758dcd66fc0c`, retaining their original license notices.
Core, SDK, protocol and secret types use that exact public revision. The main
repository's later MIT decision does not alter these earlier source terms.

The executable identifies as `github`, declares accounts, resources, groups,
memberships and grants, and supports draft 2 configured health/discovery and
draft 3 declarative setup. It does not declare authoritative identities. Provider
stdout carries only bounded NDJSON; fixed error categories and limitation codes
avoid copying provider responses or credentials into diagnostics.

## Revoking access

Revoke the token in GitHub's developer settings, or revoke its organization access
as appropriate. Deleting a local keychain entry does not revoke the token at
GitHub. Use a dedicated, short-lived token where supported. Classic `repo` scope
is broader than read-only; prefer a fine-grained token with the read permissions
above. The adapter itself only issues GET requests and never requests repository
contents, messages or documents.
