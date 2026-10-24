# Persistent agent REPL

`agent_repl::AgentRepl` owns one persistent Python process. Variables, functions,
classes, and imports survive ordinary cell errors and later cells. Cells can
compose captured-source queries, named notes, provider queries, and immutable
edit plans through a Rust-controlled session facade.

The Python executable must be an existing absolute path selected by the host,
with `ReplSpec::trust_python = true`. Arbitrary Python is trusted host execution:
it can import filesystem, subprocess, or networking modules. The private working
directory and Python isolation flags are startup hygiene; they are not an OS
sandbox. Results explicitly report `osSandbox: false` and
`programHostEffects: "not_monitored"`.

The Rust facade separately enforces registered projects, source snapshots,
read-only/query-only modes, note policies, provider selection, edit plans, and
an optional `HostShell` profile. External project contexts support queries and
cannot prepare edits or execute shell commands. A repository request cannot
select an executable, provider, or trusted project root through callback fields.

`invoke_facade(&mut Session, method, arguments, project, &Deadline)` exposes the
same independently implemented source/session methods without Python authority.
The pure adapter explicitly rejects shell execution and worker-only declaration
regex or context-manager operations. Its declaration counterpart in the worker
uses CPython regex semantics, one capture and one nonempty occurrence, then
passes a verified snapshot/hash/byte position to Rust. Symbol-based provider
queries and rename plans remain pinned to the selected symbol's source snapshot;
the captured parent file selects providers for composite Vue/TS source.
Both declaration facade names use this independently implemented Python-regex
and registered LSP pathway. Requested declaration bodies must map uniquely to
captured canonical entities and retain full, hash-checked body ranges. They do
not claim that a proprietary JetBrains engine is present.

The worker protocol bounds code, frames, output, callbacks, and queues under a
shared deadline. A fatal protocol, output, or deadline failure closes the owned
process and loses its namespace. Ordinary Python errors preserve the namespace.
Result rendering handles exact built-in containers without executing custom
`repr`, iterator, or exception properties; lone-surrogate keys are normalized.

Source edits prepare immutable guarded plans and leave originals unchanged.
Named notes use immutable revisions and transactional heads. `global/` note names
select only an explicitly registered global store; plain names select project
notes. Default and unqualified topic listings merge project notes with only the
host-registered global store. Global list entries retain their `global/` prefix,
scope and immutable revision identity, so colliding local names remain usable.
An explicit `global/` topic selects that namespace alone. A registered backend
failure fails the listing instead of returning a partial success. Combined
listings have an explicit item/byte budget. Cross-namespace rename is unavailable.

The public inventory contains 53 upstream tool names and 50 REPL facade names at
Serena revision `4f2520efb128dd02e66a2302095bdbbb26ddd5a0`. Those counts describe
contract coverage, not complete behavior or provider certification. See
[`serena-facade-mapping-2026-10-14.json`](../../docs/parity/serena-facade-mapping-2026-10-14.json)
for implemented native behavior, partial equivalents, unavailable methods, and
remaining option boundaries. Native JSON responses and guarded-plan edits differ
from upstream formatted responses and immediate edits. Independently labeled
LSP equivalents now prepare safe-delete, forced-delete, file-move and supported
inline plans, and expose provider diagnostics and declared action-kind registries.
Symbol moves and deletion propagation still require additional provider intent
support. Keeping an inline definition can project one isolated exact declaration
removal out of a provider action while retaining independently checked captured
call replacements. Explicit Rust comment/text rename options use a bounded syntax
projection. See [checked refactoring intents](semantic-refactor-intents.md) for
source scope, refusals, and the operational publication boundary. These are not
proprietary JetBrains engines.
No GPL Serena application implementation was copied.

Positive `max_answer_chars` bounds count Unicode characters in serialized native
JSON; oversized results fail explicitly without returning clipped source claims.
Numeric SymbolKind filters apply to the containing referencing symbol. Unmapped
native kinds do not receive fabricated enum codes. `include_info` requests actual
hover at the captured symbol snapshot/hash, including implementation targets and
worker-selected declaration positions. `max_matches` limits the permitted root
matches: exceeding it raises an error with a shortened result. `-1` gathers all
matches within explicit native result budgets using pinned snapshot pages.
Descendants do not consume this root-match threshold. Full symbol bodies suppress
descendant expansion and hover; hover is never appended to child symbols.
Overview depth defaults to one for Java/Kotlin and zero for other languages.
Overloaded sibling symbols receive stable zero-based `[i]` path components in
source order. Those paths select the same captured entities in subsequent
reference, body and semantic refactoring requests. Relative paths match suffixes;
leading `/` requires the full source-file symbol path. Substring matching affects
only the final component. The independent `jb` facade accepts component `*`
patterns and directs wildcard-only requests to the overview operation.
`jb.find_symbol(search_deps=True)` includes only dependencies already captured by
explicit host scope. False or omitted excludes captured read-only dependencies
before root-match thresholds; this switch never expands capture authority.
Code-only pattern searches select the proven capture languages before source
reads and matching, excluding text, configuration and Markdown. Explicit host
language overrides remain effective. Regex, context, glob and pagination behavior
reuse native search; no provider execution or wider capture is granted. Ignored
files absent from the host capture remain unavailable. Project listing reports
native symbol-query API availability and explicitly leaves live provider health
unprobed; `symbol_access` must be a boolean.
`skip_ignored_files` filters immutable observed ignore outcomes from explicit host
read-only capture scope: list/find default to false and pattern search to true.
It never expands source access. Filtering precedes reads and pagination; basename
file masks match recursively. See [ignored-source-capture.md](ignored-source-capture.md).
Implementation kind filters classify the
implemented source symbol; ambiguous or unclassified symbols fail filtered requests
explicitly. File documentation in the `jb` overview facade returns exact leading
grammar comments or Python module-docstring spans, excluding executable f-strings
and byte literals while retaining adjacent constant string expressions.

File diagnostic line bounds are zero-based and inclusive. Symbol diagnostics may
also include diagnostics for whole captured referencing symbols, as the public
contract requests. Each referencing symbol retains an immutable snapshot, hash
and UTF-8 range. This deliberately includes diagnostics elsewhere in that caller;
the captured reference scope is reported explicitly and does not claim exhaustive
compiler analysis. Full-file diagnostic and inspection ranges retain zero-width
EOF diagnostics, including empty source, while partial lines exclude the next line.

Provider restart now performs real fresh initialization, health negotiation, and
shutdown with a generation that advances only on success. It reports the current
request-scoped lifecycle and never claims to restart an unowned TCP process.
See [provider-authority.md](provider-authority.md) for typed host passive profiles,
private source selectors, real provider execution gates, and immutable overlay v2.

`jb.debug_eval` can use an independently owned persistent DAP adapter selected by
the host. Adapter launch trust, target execution, and evaluation require separate
host permissions. Evaluation uses verified stopped frames and captured source
hashes; it may execute authorized target code and is not a sandbox. A real isolated
debugpy 1.8.22 Windows gate evaluated two cells and completed owned disconnect.
No proprietary debugger engine is included.

Validation: the current grouped Windows gate passed 21 integration tests, three
option tests, four semantic-option tests, two documentation tests, four diagnostic
boundary tests, eight pinned symbol-contract tests, two dependency-scope tests
and two memory-namespace tests. A separate four-test file-option gate plus 20
native-query/Trail regressions and a private stale-source gate also passed.
They cover persistence, composition, external projects, source and
global notes, all implemented edit families, provider hierarchy/diagnostics,
Python declaration regex, semantic/composite rename, explicit shell profiles,
authority-key rejection, state loss, enforced options, and safe rendering.
REPL semantic gates use an independently owned protocol fixture. Three installed
Windows servers were validated separately in the LSP enrichment wave; this does
not certify every catalog backend or every facade method. Lib and REPL-test
Clippy passed with `-D warnings`.
