# Explicit global tool budgets

The host can select two independent global numeric policies through its JSON
profile or hash-selected global YAML. No provider, process, source path, write
permission or project registration follows from either value.

```yaml
tool_timeout: 240
default_max_tool_answer_chars: 150000
```

The equivalent JSON HostProfile selection is:

```json
{"toolBudgets":{"timeoutMs":240000,"defaultAnswerChars":150000}}
```

These public setting names and defaults are pinned to Serena revision
`ede329aaf0c824bea75455a2926f01420d7aa620` in
`native/src/tool_budgets_contract.json`. The compiled data drives field mapping
and the declared answer-facade inventory. Upstream code is neither imported nor
executed. The immutable historical tool/REPL contracts remain unchanged.

## Selection and admission

Both YAML settings are global only. Project, project-local, context and mode
definitions refuse them before invoking a renderer for that selected source.
The JSON policy rejects unknown fields and nonintegral millisecond/character
values. Both paths validate before selecting persistent Session state.

`tool_timeout` accepts a finite positive number of seconds up to 600. Positive
fractions round upward to integral milliseconds, with a minimum of one; zero,
negative, Boolean, string, nonfinite, overflowing and underflowed values refuse.
JSON `timeoutMs` accepts integers from 1 through 600000. There is no unlimited
timeout sentinel. `default_max_tool_answer_chars` and `defaultAnswerChars` accept
integers from 0 through 4000000, including a literal zero-character ceiling.
Each YAML null clears that field alone; an omitted field retains the host's
existing explicit selection. Empty/null selections serialize away. Without a
selection, old default profile bytes, bindings and tool behavior are preserved;
the public template defaults are not silently installed.

The complete nonempty policy participates in the Session binding and concurrent
state admission. Resuming an existing Session with a changed policy refuses.
`configuration`/`get_current_config` reports the selected `toolBudgets` object.
Projection receipts retain the original `toolBudgetDeclarations`. These records
describe selected data, not an execution grant or runtime certificate.

## Tool duration

The timeout surrounds public Serena tools, direct facade dispatch, trusted
persistent Python cells and their callbacks, and native Session `invoke`
operations. Serena MCP stdio/HTTP tool calls use it instead of their own
120-second default. The existing original transport/host deadline remains an
independent ceiling; selecting 240 seconds cannot widen a shorter HTTP request
or library-supplied deadline. Both elapsed admission time and subsequent nested
work consume the same outer budget, rather than restarting it per callback.

The existing Deadline intersection and host scope propagate all cancellation
flags and tighter clocks to nested work. Post-call checks refuse expired
successes. Existing owned-process cleanup still runs, and typed unproven cleanup
remains stronger than a timeout. A timed-out persistent cell loses its worker;
it cannot be reused as a successful live session.

This is the existing cooperative native deadline and owned-process lifecycle,
not a new OS scheduler quota. Synchronous native/file operations can reach their
next check after expiry. Engine APIs invoked independently of a Session,
startup/profile loading, transport discovery/resources/prompts and dashboard
callbacks invoked outside these tool entry points retain their own policies.
No hard interruptible-I/O or all-interface runtime guarantee is claimed.

## Answer characters

Only the 17 existing public facades declaring `max_answer_chars` receive the
selected answer default. Their missing argument or `-1` resolves the explicit
host default. A nonnegative per-call argument overrides that default, including
zero. Other negative values, values above 4000000, nulls, fractions, strings and
Booleans refuse. Tools without this declared argument, including configuration,
do not acquire a new per-call parameter or an implicit answer-character gate.

The limit checks the existing native serialized JSON value's Unicode scalar
count before delivery. Astral characters and combining scalars remain distinct
from UTF-8 bytes; JSON quotes/escaping count as before. It is not a token count,
grapheme count or a promise of byte-identical upstream textual output. Over-limit
results return an explicit error without clipping or pretending completeness.
Original frame, structured-output and transport byte ceilings still apply even
when the configured/per-call character limit is larger.

As with the previous per-call length check, this check follows the tool action.
It is not a transaction rollback guarantee: a tool may already have captured
data or prepared immutable plans before its result is refused. All original
source-edit approval, verification and recovery gates remain independent.

## Evidence and remaining qualification

Two inert public-data tests were observed failing before the compiled contract
was added and then passing. They check scopes, units, defaults, exact upstream
pins and all 17 declared facade memberships; they do not execute Rust.
Four native unit tests and seven integration tests are authored for numeric
admission, clears, legacy/stale binding, Unicode boundaries, exact overrides,
real stdio/HTTP, persistent callbacks, timeout/state loss, cancellation inheritance
and cleanup failure precedence. They remain unrun locally. Formatting/source
parsing, typecheck and npm results are recorded separately in the publication
receipt; none establishes native runtime correctness.

Exact hosted compilation, whole-target regression and actual deadline/cleanup
qualification remain required. Token estimators, backend auto-detection
priorities, full current upstream parity, other platform/backend acceptance,
combined VSIX, main and release acceptance remain open.
