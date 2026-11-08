# GitHub App installation authorization

This native host operation is separate from signed webhook admission. The
webhook HMAC authenticates payload bytes; an app RSA key signs app JWTs, which
authorize installation discovery and restricted token creation. Neither proves
the current PR head, source safety, review correctness or full product parity.

The synchronous Rust host API is `github_app_auth::{AuthProfile, Client,
InstallationLease}`. `Client::production` requires explicit network permission
before an OS credential read. It uses the fixed `https://api.github.com` origin,
certificate verification and API version `2026-03-10`, with no redirects,
environment proxy or automatic retries. The separate GET reader is unchanged.
Do not call this synchronous host interface from within a Tokio runtime.

## Explicit host inputs

Create an app-auth host JSON file outside analyzed source. It uses this schema:

```json
{
  "schemaVersion": 1,
  "appId": 17,
  "clientId": "Iv1.your-app-client-id",
  "keyCredential": "review-app-key",
  "grants": [
    {"installationId": 31, "repositoryId": 47, "repository": "org/repo"}
  ],
  "permissions": {"contents": "read", "pull_requests": "write"}
}
```

These are illustrative IDs, not a registered installation. `clientId` is
optional; its absence selects the documented numeric app ID issuer. Grants use
canonical lowercase GitHub repository names and explicit positive numeric IDs,
with at most 32 distinct installation/repository pairs. Profile fields and
object keys are strict. The supported minimal permissions are `contents:read`
and `pull_requests`, `issues` or `checks` at `read` or `write`. Supply only the
permissions the eventual host operation needs. A host profile does not verify
remote installation ownership by itself.

Import an authorized private key through a private stdin pipe:

```text
yoke credentials github-app-key-import review-app-key
```

The command requires redirected stdin, never a key in argv or an echoed
terminal. The host must construct the private binary/text pipe. Accepted inputs
are unencrypted PKCS1 `RSA PRIVATE KEY` or PKCS8 `PRIVATE KEY` PEM, or DER.
Input is bounded to 8192 bytes, decoded private DER to 2560 bytes, and RSA keys
to 2048–4096 bits. The decoded DER is written create-only under
`yoke-code-intelligence/service/github-app-key/NAME`, distinct from bearer and
webhook credentials. No private key, fingerprint, JWT or token is printed.

```text
yoke github-app --allow-github authorize --profile HOST_AUTH_JSON --installation 31 --repository 47
```

This host-only command opens no source Store and does not capture source. Source
root/snapshot/provider flags, webhook permission and workspace write permission
are unavailable for this operation. Without `--allow-github`, permission fails
before profile/credential/source access. Installation and repository IDs must
match a configured grant.

## Protocol and authority

RS256 JWTs bind the selected issuer, `iat=host_now-60` and `exp=host_now+540`.
Authorization performs exactly four requests: authenticated app identity,
installation identity and suspension/permission checks, a token POST restricted
to the one repository ID and requested permissions, and installation repository
enumeration using that new token. App JWTs authenticate the first three requests.
Both token-response and enumeration repository IDs/names must match exactly;
the enumeration must contain exactly one repository. Token permissions must
equal requested permissions, allowing only GitHub's automatic `metadata:read`.
The installation itself may have broader permissions than the restricted token.

Token text is opaque and header-safe, bounded to 16384 bytes without a fixed
length/prefix assumption. Provider-reported expiry must be 30–3660 seconds after
the initial host time, with at least 30 seconds still remaining after the final
response. The host clock and HTTPS provider responses are trust boundaries.
The returned lease is opaque in memory; its public evidence contains IDs,
checked permissions/expiry, byte/request counts and origin/fixture labels.
Neither tokens nor token hashes enter JSON or the source Store. The CLI drops
the lease after printing public evidence; later worker/publication integration
must revalidate lease time, scope and current provider/source preconditions.

The request budget must admit all four requests before I/O. Per-response raw
body and aggregate header/body budgets, output including final LF and inherited
deadlines are checked. Zero limits refuse before I/O. Response bytes, duplicate
JSON, changed identity, extra token permissions, redirects, encodings and
pagination links fail closed. Request/body futures poll inherited cancellation
and expiry every 10ms and are dropped on abort without retry. This cooperative
protocol is not an OS scheduler or preemptible system-DNS guarantee.

The token POST is a real remote side effect: a token may be minted even if a
later response, repository or output check fails. The operation does not claim
remote atomicity, revoke-on-error, source execution or publication. Library and
transport allocations are not an all-memory-zeroization attestation.

## Validation boundary

`FixtureProfile` is an explicit in-process, numeric-loopback capability with
owned ephemeral RSA material and optional host-controlled fixture time. It
cannot load a production OS key, and JSON cannot select its endpoint or clock.
Fixture evidence is labeled; passing a local protocol fixture does not establish
a live GitHub installation. Windows OS-import checks use only owned temporary
credential entries and verify their deletion. No real app, webhook endpoint,
comment, check or deployment is created during those checks.

Signed [HTTP/TLS ingress](github-app-ingress.md) is a separate host operation.
Worker claim/completion, publication and discussion digests remain separate R8
work. Universal parity and live installation certification remain open.

Primary protocol sources: [GitHub JWT generation](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app),
[private-key management](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/managing-private-keys-for-github-apps),
[installation access tokens](https://docs.github.com/en/rest/apps/apps#create-an-installation-access-token-for-an-app)
and [installation repositories](https://docs.github.com/en/rest/apps/installations#list-repositories-accessible-to-the-app-installation).
