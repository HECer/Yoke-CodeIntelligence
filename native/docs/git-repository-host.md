# Explicit fixed-root Git-scoped native MCP

```sh
yoke --root /absolute/project git repository-scope --capture --serve --allow-index-read --trust-git
# Optional: --git /absolute/trusted/git; explicit follow/no-follow flags;
# the repository-scope finite limits remain available.
```

This host-selected command completes its initial Git-index/working-source capture
before accepting newline JSON MCP. It emits no capture-summary JSON on protocol
stdout. Cold metadata/physical preflight precedes Store creation. A client process
configuration can supply the same exact argument array; do not put permissions or
program paths in tool arguments or repository content.

The native eight-tool catalog and protocol version remain unchanged. Ordinary
`serve`, ordinary `index` and metadata-only `repository-scope` keep their preceding
defaults. This explicit server starts with no project source-write permission.
It neither starts a model nor executes analyzed source. Host Git remains a trusted
program running under the host account, not an OS sandbox.

The invocation owns a non-deserializable driver binding its canonical root,
trusted Git program, finite options and the observed host traversal preference.
The CLI timeout bounds startup; later calls retain their existing bounded MCP
request timeouts. Git/source budgets remain independent of response byte budgets.
Both normal indexing and the bounded retrieval indexing route refresh Git metadata
and working-file capture through the same selected policy. Freshly discovered
working files can enter the next capture; source/index observations stay separate.
Saved graph capacity errors remain typed failures, without unbounded fallback.

The last successful effective source selection also remains current host policy.
Included crossed repositories stay read-only, including when a caller selects an
older snapshot that originally allowed writes. Captured policy, current scope,
ordinary approval/verification and journal recovery remain separate gates. The
trusted Rust API can install the driver on an existing Engine while preserving
its other host-selected source options and independent permissions.

Every native tool call checks the bound current preference without starting Git.
A changed, cleared or newly appearing preference refuses as stale; restart with
an explicitly selected current binding. Per-invocation follow/no-follow overrides
do not rewrite saved preferences. Client JSON cannot change the root, Git program,
trust, follow choices or Git limits. A stored report/snapshot is never authority
to create this driver.

Saved reads need no live Git when the host binding is still current. New indexing
requires actual current Git observations and refuses when Git administration is
unavailable. Original snapshots remain immutable. Cooperative preconditions and
double scans are not OS-atomic isolation against hostile same-account mutation.

The qualified scope is one fixed root and the native stdio profile. An actual
native protocol client performs indexing, and pinned Claude 2.1.306 connects to a
separately owned configuration without a model turn. Session/multiproject/HTTP,
other facade/client adoption, broader formats/quality and whole Windows/upstream
certification remain separate work. See [capture](git-repository-scope.md) and
[host preferences](git-repository-preferences.md).
