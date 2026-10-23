# Explicit live session prompt templates

A host may independently enable `promptTemplates` on a Session profile. At each
MCP initialization or `initialInstructions` call the native renderer derives the
current transport's actual selected tool catalog, then renders the selected
system prompt and effective context/mode/global/project/local/CLI fragments.
Mode and active-project changes affect the next rendering. No lookup, model call,
source execution, file loader or upstream prompt text is enabled by default.
The core is MIT; only pinned public tool/category interface names are retained.

## Host selection and independent permission

Example host JSON (paths and byte hashes are supplied by the host):

```json
{
  "context": "desktop-app",
  "modes": ["planning"],
  "promptTemplates": {
    "context": {
      "variables": {"subject": "owned project"},
      "templates": {
        "system_prompt": "For {{ subject }}: {% for name in available_tools %}{{ name }} {% endfor %}"
      }
    },
    "systemPrompt": "system_prompt"
  },
  "steering": {}
}
```

`context` accepts the same explicit inline definitions, original-byte-pinned YAML
catalogs and collision policy as [prompt catalogs](prompt-catalogs.md).
`systemPrompt` is an optional template name, never a path. Without it the existing
native base instructions precede the rendered steering fragments.

```text
yoke --root C:/owned/source --allow-session-templates serve-session
  --profile serena --id owned --host-profile C:/host/profile.json
```

The separate global `--allow-session-templates` grants the fixed native callback.
It is valid only for session, serve-session, serve-http and agent-repl commands.
Supplying JSON alone cannot grant rendering or choose an executable. Catalogs
are admitted only after the grant, pinned, held and rechecked before and after
rendering. Changing selection/marker metadata changes the session fingerprint;
resuming an incompatible persisted session refuses.

HTTP requires its existing `--trust-service` grant as well. Select a principal
with profile `session` for the native Session catalog, or serena/graphify/trail/
graft for those adapters. Plain HTTP profile `native` is a stateless Engine and
refuses session templates. Valid host admission may create initial `.yoke` state
before opening the listener; an unauthenticated request cannot create a transport
session. Persistent Python additionally requires its own explicitly selected
host interpreter and trust; its prompt profile is Serena. Rendering itself never
starts Python. Normal CLI batch Sessions use the native Session catalog.

## Derived values and provenance

The host owns three reserved variable names. User data cannot override them:

| Name | Value |
| --- | --- |
| available_tools | Sorted exact names in the selected live transport catalog. |
| available_markers | Sorted category names for the selected tools only. |
| tool_names | Canonical name to effective transport name over the full catalog. |

For Native/Serena/Graphify, canonical names retain the registered tool names.
Trail/Graft maps bare adapter names to their actual trail_/graft_ names. A mapping
can describe a currently excluded tool; availability comes from available_tools.
No hypothetical provider operation is added to the actual catalog.

Serena marker metadata pins public category names and inherited marker
relationships at revision 4f2520efb128dd02e66a2302095bdbbb26ddd5a0. All 53 own
compatibility names are checked against that public API. No upstream application
methods, algorithms or prompt text are copied. Native aggregate categories are
explicitly scoped to own registered entries; the other adapters are untagged.
Categories grant no authority and do not certify actual external provider
readiness, effects, execution or upstream semantic equivalence.

`promptRendering` binds the exact session-state hash/revision, active project,
context/modes, config fingerprint, actual full/selected catalog hashes, profile
and marker metadata. Every fragment retains its source label and worker receipt.
Original YAML/raw bytes, decoded scalar hashes and rendered hashes remain
separate; raw scalar byte spans are unavailable. `complete` stays false.
Missing names/values, stale pins/state, schema or render failures refuse the
response without partial instructions; a failed initialize remains uninitialized.

The retained library `Session::instructions()` returns literal guidance for API
compatibility. Dynamic hosts call fallible `prompt_instructions(deadline)`.
Library callers must explicitly construct nonserializable `Runtime::trusted`
with their own renderer; that callback owns its execution/resource guarantees.
JSON cannot construct or restore callback authority. The shipped CLI uses the
fixed guarded native worker described below.

## Deferred YAML projection

Eager `session-config plan/export` keeps its preceding literal output semantics.
To retain decoded template scalars for later live rendering, add templateContext
and optional systemPrompt to the policy selection, then pass:

```text
yoke session-config export --selection C:/host/selection.json
  --allow-configuration-read --allow-policy-projection
  --allow-template-evaluation --defer-template-evaluation
  --allow-output --output C:/host/new-profile.json
```

The source YAML authority/policy fields are validated before projection. The
reply explicitly records `templatesDeferred: true`, `templatesEvaluated: false`
and startup authority still required. Original decoded prompts are retained,
with deferred receipts and exact config/pin provenance. The create-only JSON
profile contains data only; it does not carry a trusted renderer. Starting it
still needs the independent `--allow-session-templates` grant.

Discovery accepts the same deferred selection and returns guarded replayable
catalog pins. Replaying its ready selection requires the deferred flag again.
`systemPrompt` without deferred projection refuses. Mixing eager evaluation with
an already dynamic base profile refuses instead of silently evaluating twice.

## Bounds and qualification limits

Protocol 3 carries resolved inert data plus a bounded runtime binding. It uses
at most 8,000,000 request bytes, 128,000 variable/binding bytes each, 256 effective
names and the existing source/declaration bounds. Every fragment has an 8,192-byte
UTF-8 output ceiling; combined instructions are at most 131,072 bytes. The native
worker retains 100,000 fuel, recursion 64, checked 256MiB parent/self OS memory,
a 10-second ceiling and inherited deadlines/cancellation. Its reply binds the
canonical runtime-binding hash, request, resolved data and decoded/rendered text.
Old protocols 1/2 reject runtimeBinding, including null; default/literal/eager
serialized bytes and absent-setting identities remain unchanged.

The worker exposes inert JSON/catalog strings only, with no environment/file
loader, application object, network, command or model interface. These are
resource/interpreter boundaries, not an OS filesystem/network sandbox or
protection from another equal-account process.

Default/upstream prompt catalogs, embed_memory, exact Python-Jinja conformance,
external layouts/extensions/clients and broad parity remain open. Windows-owned
qualification does not certify Unix, current whole Windows, hosted CI, canonical
VSIX, Google live capture, main integration or release. See the
[exact source-bound evidence](../../docs/validation/integration-2026-10-23/runtime-prompts/README.md).
