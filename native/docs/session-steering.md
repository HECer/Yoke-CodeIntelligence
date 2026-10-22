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
arbitrary Jinja templates, automatic activation commands, complete client
equivalence and the remaining 25 project settings are not certified by it. In particular, repository instructions never authorize code execution.

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

## Explicit startup project

The host profile optionally accepts startupProject, an exact registered name
or default for the primary root. A new session selects it before resolving
project/project-local instructions, modes and tool restrictions. Unknown names,
paths, confusables, overlong labels and query-only registrations refuse before
session publication. No project source setting is loaded or trusted by this
field, and project selection never executes an activation command.

With ide, claude-code or grok, an explicit startup selection fixes the active
project for that session. activate_project is hidden and refused; direct
session activate and generic Python callbacks also refuse, even when selecting
the same project. Removing the fixed project is refused. Modes remain mutable
under their existing policy/CAS gates and cannot unlock project activation.
These native contexts implement the documented single-project startup behavior;
[the pinned public configuration contract](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/docs/02-usage/050_configuration.md#contexts)
is the functional reference, not copied application code or prompt text.

Other contexts use startupProject only for a newly created session; they retain
ordinary project switching, and resume retains the current saved project.
Changing the host startup selection or context changes the binding and refuses
an old session. Fixed-project resume additionally rejects a stored active
project which differs from the pinned selection. Host registration, independent
query-project scopes, source access, read-only and execution grants still apply.
This fixes active-project steering and does not create an OS or source sandbox.

configuration adds startupProject and projectActivationLocked only when the host
selected the option. With no selection, serialization, fingerprint domain,
configuration and existing activation semantics retain their original bytes.
The primary --root remains the session-state owner; startupProject chooses the
active registered project for session-backed operations. The independent native
eight-tool profile retains its existing primary-Engine contract.

## Named-memory regex policies

Every host steering layer and custom context/mode definition accepts optional
readOnlyMemoryPatterns and ignoredMemoryPatterns arrays. They accumulate in
layer order with identical patterns deduplicated across layers. These are Rust
regular expressions, not globs. The existing notesPolicy.readOnlyPatterns and
ignoredPatterns keep their original glob semantics; both sets of restrictions
apply. Neither form can turn a non-writable host note policy into a writable one.
The [pinned public settings template](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/serena_config.template.yml)
supplies the functional reference; no application implementation is copied.

New regexes match the canonical logical memory name: project names have no
prefix, global notes match global/name. For example, ^keep[0-9]$ applies to
project keep1, while ^global/keep[0-9]$ applies to its global counterpart.
Unicode names and patterns remain exact. Matching uses ordinary regex search;
anchors are explicit. An empty expression intentionally matches every name.
Unsupported lookaround/backreferences, invalid expressions, duplicate entries
within a layer and NUL strings refuse rather than fall back to literal/glob mode.

Ignored notes are absent from lists and cannot be read or mutated. Read-only
notes remain readable with readOnly metadata, while write/edit/delete and both
rename endpoints refuse. Rename reference updates preserve hidden and read-only
revisions; hidden names are not exposed in the result. Original note heads,
immutable revisions, source snapshots and anchors are not migrated or rewritten.
The native Store remains separate from central AgentMemory and source files.

Limits are 100 patterns per category, 4096 UTF-8 bytes per expression and 32768
combined pattern bytes for a layer or effective policy, within the existing
256 KiB host steering bound. Each compiled set has 1 MB expression/DFA-cache
limits. Complete merged policies validate before session creation, resume or
mode/project publication. Tool catalogs use bounded metadata without repeatedly
compiling regexes for every tool. Note operations compile the effective sets
before entering their existing transaction; ordinary source/write/host gates
remain independent.

A registered memory policy checks the exact persisted state before direct note
operations and before REPL cells/callbacks. Same-cell mode changes immediately
affect cached memory functions. A denied callback preserves the worker namespace;
an external stale-session error retains the existing worker-close behavior and
requires a resumed session plus a new explicitly authorized worker. Rejected
cells do not execute. Empty fields are omitted, preserving prior serialized
bindings, schemas and real legacy results.

This is a bounded native regex contract. Python-regex equivalence, automatic YAML
loading, arbitrary upstream file access and all remaining settings are separate
requirements. Repository data never selects this policy or grants authority. Independent host
CLI use and same-account database access remain outside session steering.

## Fixed MCP tool bases

Every host layer and custom context/mode definition optionally accepts fixedTools.
A nonempty list replaces the default base; an empty list leaves the preceding
base in place. With no selected fixed base, the preceding native catalog remains
unchanged. Resolution order is global, active project, project-local, fixed
context, each active mode in resolved order, then CLI. The last nonempty base
wins. configuration.steering exposes fixedTools and fixedToolsSource only when
a base is selected. The [pinned public settings template](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/serena_config.template.yml)
supplies the functional base-selection/conflict reference, not application code.

Names must be exact known native/session/Serena/Trail/Graft/Graphify tool entries
or recognized original query aliases. Unknown names, wildcards, paths, Unicode
confusables and duplicates refuse. A layer or custom definition cannot combine
nonempty fixedTools and excludedTools. Exclusions from other layers still apply,
including built-in context/mode and fixed-startup-project restrictions. A base
never re-enables an excluded tool. Existing host-policy size/tool-count bounds
apply before session publication.

The six original Trail/Graft query identities cover both prefixes and all their
legacy aliases. Selecting graft_find_code therefore selects the same original
find-code operation through trail_find_code or graft_ask. It does not select a
different query family. Other tool names keep their exact entry identity. Each
session-backed MCP profile filters its original definitions without rewriting
schemas or inventing unavailable entries. The independent native eight-tool
profile retains its primary-Engine contract.

Tools/list, dispatch and direct adapters check a registered fixed policy against
the exact persisted session record. This also applies when the current mode has
no fixed base but another registered mode does. A parallel mode/project change
requires resume; stale initialization/discovery/call returns an explicit error,
not an empty complete catalog. Real catalog changes retain the existing bounded
stdio/HTTP list-change notifications. Source-backed session operations and
engines also reject stale fixed-policy sessions before source work.

This selects MCP tools. It neither grants nor replaces Python API selection;
an explicitly trusted host REPL keeps its separate API/operation gates. A visible
edit tool may prepare an immutable review plan while application still requires
its original host permission, approval and stale/recovery guards. Read-only
project and fixed-startup restrictions remain enforced. There is no source,
shell, model, runtime trust or source-write permission derived from a tool list.
Interface selection and optional tool/API inclusion remain separate settings;
unsupported fields refuse instead of being accepted without an effect.

Empty fields are omitted so preceding serialized profiles, fingerprints,
configuration, schemas and actual raw CLI/MCP results stay byte-identical.

## Fixed startup agent interface

HostProfile.agentInterface and global/CLI/project/projectLocal steering layers
accept exactly tools or repl (lowercase native JSON). Absent or null values keep
the preceding behavior and omit all new serialized fields. Context and mode
definitions reject agentInterface, because it cannot change during a session.
Resolution is host.agentInterface, global, CLI, startup project, startup
project-local; the last non-null value wins. The selected startup project is
host.startupProject, or default. Later activation, mode changes and resume retain
that startup selection. configuration.steering exposes agentInterface,
agentInterfaceSource and agentInterfaceProject only when explicitly selected.
Changed host selection refuses an existing host-bound session.

Without optionalDefaults, tools uses the original 52 non-serena_repl candidates under the
existing fixed/excluded tool rules and context/startup restrictions. With repl,
the six original no-facade control definitions are candidates: activate_project,
remove_project, query_project, serena_repl, initial_instructions and serena_info.
Fixed/excluded tool lists do not filter this REPL view. Startup activation
restrictions and every operation's original arguments and permission checks
remain effective. serena_info retains its topic-based native information
contract; it is not a promise of complete upstream information equivalence.

The setting selects no executable, starts no worker and grants no trust. Calling
serena_repl still requires its original host-owned session_id and a separately
configured/trusted Python runtime. Without one it returns an explicit error.
excludedApis remains independent and is enforced on real cached callbacks; tool
steering does not become API authority. Python is a trusted host interpreter,
not an operating-system sandbox. Source application permissions, immutable
plans, approval/stale/recovery guards and explicit execution gates remain intact.

Opted interface sessions check their exact current persisted record before
catalog admission, direct adapters, source operations and Python cells. External
changes require resume; a stale worker closes before its cell executes. Other
session profiles keep their original catalogs, and independent native eight-tool profile is
unaffected. No-setting sessions retain all 53 preceding mixed definitions,
fingerprints, raw CLI/MCP output and complete logical Store SQL.

The fixed-startup/interface distinction follows the pinned public
[configuration guide](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/docs/02-usage/050_configuration.md)
and [settings template](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/serena_config.template.yml).
This is an explicit native JSON contract. Optional tool/API inclusion and default
subsets, upstream YAML/Jinja loading and all project/client settings remain open;
unknown fields refuse. No upstream application code or prompt was copied.

## Explicit tool and API inclusions

Layer and custom context/mode definitions accept includedOptionalTools and
includedApis. They are explicit host settings, not permissions inferred from
analyzed files. Selectors use the existing exact known native tool entries,
recognized original query aliases, and the 50 declared API methods/eight groups.
Unknown, duplicate, wildcard, path, Unicode-confusable and oversized selectors
refuse. A layer/definition cannot combine a nonempty fixedTools with nonempty
includedOptionalTools or excludedTools. Existing empty fields remain omitted.

Native resolution order is global, active project, project-local, fixed context,
each active mode in order, then CLI. Each layer applies exclusions, its fixed
base if any, then its inclusions. A tool inclusion removes matching earlier
exclusions and extends an earlier fixed base; later exclusions can remove it
again and a later fixed base replaces that base. Original Trail/Graft query
identities apply across both prefixes and legacy aliases. Startup activation
restrictions apply after these advisory settings and cannot be re-enabled.

API selectors expand against the declared native methods. An individual method
can be included inside an excluded group, without enabling its other methods.
A later group/method exclusion can disable it again. Existing exclusion-only
selectors keep their original serialization. configuration.steering.inclusions
records applied steps in order with source, tools and apis. It is provenance;
the final excludedTools/excludedApis/fixedTools and catalogs describe the effect
of all later steps. fixedToolsSource retains the original fixed base's source.

Real MCP discovery and dispatch share the effective tool selection. Python
per-cell discovery and every current callback share the effective API selection,
including cached references and changes activated by a cell. API/tool selection
remain independent; an ordinary trusted tool adapter retains its separate
origin. A registered inclusion in an unused mode still enables exact current
persisted-state checks before catalogs, direct adapters and cells. An external
parallel update requires resume and closes a stale worker before execution.

Empty inclusions preserve preceding profile/fingerprint/raw bytes, the original
53-schema mixed view, explicit 52-candidate tools view and six REPL controls.
The explicit REPL view continues to ignore tool-list settings; API settings
still apply. No inclusion selects/trusts Python, starts an interpreter, grants
source application rights or bypasses read-only projects, approvals, immutable
plans, stale/recovery guards or fixed-startup constraints. Python is not an OS
sandbox. New fields require a new matching host-bound session.

These are documented native layer rules and explicit inclusion semantics.
Optional default subsets and complete upstream YAML/configuration/client
selection equivalence remain open; this checkpoint does not silently change the
preceding defaults or certify the original 353-row inventory.

## Versioned optional defaults

HostProfile.optionalDefaults accepts exactly serena_optional_v1, or absent/null
for the preceding native behavior. This host-only startup selection is immutable
for a session and rejects unknown/case-confusable/type values and nested context,
mode or project-layer attempts. Its selected version and actual optional arrays
are bound into the host fingerprint. Existing sessions require a matching host
binding; upgrade or downgrade requires a new session. Source snapshots retain
their original independent source/analysis identities.

The dated native projection uses the public
[Tools and APIs documentation](https://oraios.github.io/serena/01-about/035_tools.html)
from 2026-10-22, HTML SHA256
8988989c30bf33ecce4bfab1d8e982e8413950428f8af421293624288a558b59.
All 53 tool names match the original contract. Of those, 24 are marked optional,
leaving 29 classical defaults. Eight methods of the original native 50 are
optional: edit.delete_lines/insert_at_line/replace_lines, all three ext methods,
and lsp.get_diagnostics_for_symbol/restart_language_server. They leave 42 native
default methods. Current documentation's 14 additional ide methods are outside
this projection. This is current published flag metadata, not a certification
of optional flags at the original pinned revision or every backend.

The versioned optional tool filter applies only to Serena discovery and direct
compatibility dispatch. Other session profiles retain their original schemas;
the independent native eight-tool profile is unchanged. Normal tool exclusions
still apply. An explicit fixed base replaces the optional default base; layered
includedOptionalTools can enable optional entries. Later exclusions can narrow
them again. A migrated classic/tools session can explicitly include serena_repl,
which remains subject to a separately configured/trusted host Python runtime.
Without the migration selection, preceding tools still excludes serena_repl.

The explicit REPL interface retains its six original controls, independently of
optional tool filtering and fixed/excluded/included tool settings. Optional API
methods begin excluded before global/project/project-local/context/mode/CLI
selection. Existing includedApis can restore methods or entire groups; later
exclusions can remove them again. Per-cell discovery and every callback enforce
the current selection, including callables cached under an earlier unrestricted
session. A registered selected default policy requires exact current persisted
state before catalogs, adapters and cells; external changes require resume.

configuration.steering.optionalDefaults exposes the selected version and
excludedApis exposes effective API restrictions. Missing or null selection omits
new fields and preserves preceding profile bytes, fingerprints, all 53 mixed/
52 tools/six REPL schemas and 50-method discovery. Nothing silently migrates.
No selection starts/trusts an interpreter, grants source application rights,
changes read-only/startup restrictions or bypasses approval/stale/recovery guards.
The trusted Python process is not an OS sandbox. Full backend/client/YAML/Jinja
and original 353-row inventory acceptance remain separate work.
