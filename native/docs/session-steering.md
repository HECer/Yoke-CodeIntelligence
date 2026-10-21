# Host-owned session steering

Session contexts now influence native instructions and MCP tool selection. The
independently authored presets use the 16 pinned context names. `desktop-app`
retains the complete catalog; `claude-code`, `ide` and `grok` omit the duplicate
file/text/shell entry points listed in `configuration.steering.excludedTools`.
Semantic tools remain available. These are native presets, not copied upstream
prompts or certification of every client integration.

The default `agent` profile without `steering` preserves its original serialized
host/session identities, instructions and tool catalog. Other recognized contexts
enable the versioned policy. Existing sessions under those contexts refuse a
changed host binding; select a new session ID rather than replacing old records.
Unrecognized legacy context labels retain their former behavior unless steering
is explicitly enabled, in which case the host must define the context.

An optional `steering` object in the explicitly selected host profile enables
custom definitions and layered mode selection:

```json
{
  "context": "agent",
  "steering": {
    "baseModes": ["interactive"],
    "global": {"defaultModes": ["editing"], "instructions": "Keep source evidence."},
    "projects": {
      "default": {
        "project": {"addedModes": ["review"]},
        "projectLocal": {"excludedTools": ["execute_shell_command"]}
      }
    },
    "modes": {
      "review": {"instructions": "Explain the reviewed changes."}
    },
    "contexts": {},
    "cli": {"defaultModes": ["planning"]}
  }
}
```

`cli` represents explicit invocation overrides supplied by the host profile; it
is not an additional command-line flag or a file discovered in the repository.
The project map accepts only registered names, including the primary `default`.
The implementation never discovers or executes repository configuration.

Mode defaults resolve in order: existing host `modes`, global, project,
project-local, then CLI. The highest explicitly supplied `defaultModes` wins.
The effective ordered set is `baseModes`, selected modes, then the union of
`addedModes` from global, project, project-local and CLI. Duplicate membership
across layers is retained once; duplicates within a selection are rejected.
Project activation and removal recompute the result. A runtime
`switchModes {"modes":[...]}` overrides the default selection and persists across
activation/restart; base and added modes remain active. `switchModes
{"reset":true}` returns to current layered defaults. The two arguments are
mutually exclusive. Empty selection requires a nonempty effective mode set.

Instructions compose in global/project/project-local/context/active-mode/CLI
order. `configuration.steering` exposes the selected-default origin, effective
modes, exclusions and each literal fragment with its layer name. Prompts are
plain UTF-8 text: template expressions, memory references and shell syntax are
never evaluated. Custom contexts/modes cannot redefine built-in names. Host
definitions can be created, edited or removed by selecting a revised profile and
a new session ID; remote tools cannot edit the registry or change fixed context.

Tool exclusions use names from the native catalogs. Trail/Graft's six query
identities also include both prefixes and their six existing legacy aliases;
excluding any spelling excludes that same query through all its spellings.
They accumulate across
all layers and definitions; later layers cannot re-enable an exclusion. Both
`tools/list` and `tools/call` apply them in Session, Serena, Graphify, Trail and
Graft profiles. Direct adapters check before source capture or an authorized REPL.
The policy selects MCP entry points, not equivalent operations reachable through
another explicitly available entry point. An arbitrary host-authorized Python
REPL remains arbitrary host execution. Native eight-tool `serve` is independent.

The nine built-in modes supply native guidance. Existing planning/editing,
no-memories and no-onboarding session gates remain enforced. Under steering,
planning additionally omits source edit tools, no-memories omits note tools and
no-onboarding omits onboarding. Other modes are guidance and do not fabricate
benchmark results, execute tasks, activate projects or grant permission.

Steered MCP connections advertise `tools.listChanged`; stdio emits a notification
only when an operation actually changes the effective catalog. Single-message
transport hosts drain the bounded, coalesced `take_notifications()` queue.
Authenticated native HTTP delivers pending notifications followed by the JSON-RPC
response as finite `text/event-stream` events on that request. Ordinary responses
retain JSON framing. No long-lived SSE channel or GET subscription is added.
Clients should obtain current guidance through `initialInstructions` after a
mode/project change; initialization instructions are an initial observation.

Host policy binds the session fingerprint and recomputed persisted modes. Stale
host definitions, forged effective modes and concurrent state changes refuse.
The core approval, read-only project, write, execution and provider gates are
independent. Neither literal instructions nor a selected mode grants authority.

Bounds are 32 custom modes and 32 custom contexts, 101 project layers, 41 active
modes, 8 KiB per instruction fragment, 256 KiB serialized steering and 128 KiB
expanded instructions. Names are exact bounded ASCII labels; instruction Unicode
bytes remain intact. Unknown/duplicate tools, unknown settings, NULs and exceeded
bounds fail before session publication or a mode update.

This qualifies the native steering mechanism. Upstream YAML file discovery,
arbitrary Jinja templates, automatic activation commands, client-specific
single-project startup and the remaining 25 project settings are not certified
by it. In particular, repository instructions never authorize code execution.

## REPL API exclusions

Each layer and custom context/mode definition additionally accepts
`excludedApis`, a list of exact declared facade names (`mem`, `lsp`, etc.) or
methods (`mem.read_memory`, `lsp.find_symbol`, etc.). Exclusions accumulate across
the same six layers and are exposed as `configuration.steering.excludedApis`.
Unknown, duplicate, wildcard or visually confusable names refuse. Empty fields
are omitted from serialization, preserving prior host/session identities and
legacy configuration/results when no API exclusions are selected.

The host supplies the current selected method names before each explicitly
authorized Python cell. Group/method discovery reflects that cell's selection;
stored namespace objects also consult it. The host checks every callback again,
so retained callables and edits to Python discovery globals cannot bypass an
excluded API. Selection changes during a cell apply immediately to callbacks;
discovery refreshes on the next cell. Registered API policies also check the
exact persisted session revision before executing a cell and before callbacks.
Resume after an external session change.

Declaration-regex and project-context callbacks retain their original facade
identity through qualified private bridge names. A project-context entry checks
both its constructor and read-context APIs, including a manager cached before a
mode change. Ambiguous legacy private callbacks refuse while restrictions are
active. The generic native `s.invoke` helper is hidden and unavailable under an
active API restriction; use selected declared APIs. This prevents that helper
from bypassing the declared selection.

API selection applies to REPL cells. Ordinary public tools retain their separate
tool selection. Host-generated declaration/shell adapter cells use an internal
tool origin that callers cannot provide through MCP arguments; their tool and
independent host execution/source gates remain mandatory. The next user cell
restores its own API selection.

Excluding every API produces explicit empty discovery while ordinary trusted
Python computation remains available. API restrictions are steering, not an OS
sandbox. They never grant Python, shell, debugger, provider or source-write
authority. Optional API inclusion and fixed interface/tool-set selection remain
separate requirements; unsupported settings are rejected rather than accepted
without effect.

On Windows, starting an explicitly trusted Python worker first classifies the
selected program as a 32-/64-bit Windows executable. Invalid image bytes refuse
before process creation; explicitly selected `.cmd`/`.bat` launchers retain their
existing managed path. Pure host registration still executes nothing and does
not launch an image to determine its type. This uses
[Windows executable classification](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getbinarytypew)
and changes no system or privilege settings. Classification is not a sandbox or
proof of arbitrary valid-program startup behavior.
