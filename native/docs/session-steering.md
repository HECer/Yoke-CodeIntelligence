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
