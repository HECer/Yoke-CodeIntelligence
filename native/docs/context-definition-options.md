# Selected context definition options

The separately pinned
[current context template](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/resources/config/contexts/context.template.yml)
has ten public fields; the corresponding
[mode template](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/resources/config/modes/mode.template.yml)
has seven. The native candidate adds the four missing context fields and accepts
the documented null prompt. Its compiled
[field contract](../src/context_options_contract.json) binds both Git blobs and
the complete public key sets. This records public interface facts; it contains
no upstream application implementation.

Existing native tool/API exclusions, fixed tools and explicit optional inclusions
remain on their original paths. The new options belong to explicitly selected,
hash-pinned custom context definitions. Built-in contexts cannot be redefined.
Context/mode names, file/ancestor checks, YAML alias/tag/duplicate/merge refusal,
source pins, renderer authorization and profile budgets remain unchanged.

```yaml
description: Literal context metadata
prompt: null
excluded_tools: [execute_shell_command]
included_optional_tools: []
fixed_tools: []
excluded_apis: []
included_apis: []
tool_description_overrides:
  read_file: Read the selected captured source
single_project: true
structured_tool_output: null
```

`description` is literal metadata, retained in the selected native definition.
It is supported for context and mode YAML; null means empty metadata. It is not
a prompt, template, path or execution grant. A null context/mode `prompt` becomes
empty instructions and never invokes a selected prompt renderer. A nonnull prompt
retains the original literal/rendered/deferred handling and independent grants.

`tool_description_overrides` maps exact public Serena tool names to literal text.
It changes only descriptions in the selected Serena discovery catalog. Input
schemas, tool identities, annotations and permissions are untouched. Excluded
tools remain excluded, including a tool named in the override map. Unrecognized
names, nonstring values and NUL refuse. At most 53 overrides, 8,192 UTF-8 bytes
per description and 32,768 combined name/text bytes are admitted. The existing
steering and profile byte limits also apply. No clipping occurs.

`single_project` is a boolean. When true **and** the host has independently
selected `startupProject`, the existing fixed-startup-project path excludes
activation and rejects direct activation and incompatible stored Session state.
The context cannot choose that project or register another root. Without a host
startup selection, this flag does not invent a project selection. False keeps
ordinary project switching among independently host-registered projects. Earlier
built-in context behavior and all existing activation/removal/reopen checks are
retained. Changing selected context data changes the normal host fingerprint;
stale Sessions are not migrated or silently rebound.

`structured_tool_output` affects Serena-profile MCP tool replies:

| YAML value | Native behavior |
| --- | --- |
| `true` | Advertise an object output schema and return `structuredContent`. |
| `false` | Use the preceding text-only tool reply without an output schema. |
| `null` | Infer from native Serena's JSON-object transport return type, selecting structured output. |
| Absent | Preserve the preceding native catalog and text-only transport exactly. |

Explicit null is retained separately from absence through profile serialization
and replay. This inference describes the native object-envelope implementation;
it is not a certificate for another SDK's renderer or every client release.
The original JSON text remains present and decodes to the **same whole object**
as `structuredContent`, including status, source bindings and errors. `isError`
keeps its original meaning. Tool-result JSON counts both copies and their actual
escaping against the existing 4,000,000-byte compatibility inline cap. A larger
combined result refuses through the existing JSON-RPC error path; it is not
returned as clipped success. Tool execution can precede a response-size failure,
as with the existing post-call budget check; this does not make calls atomic.
Other MCP profiles, resources and prompts retain their preceding output format.

Tool-description, single-project and structured-output fields are context-only.
Mode YAML, project/local/global YAML and unsupported value types refuse. Direct
host mode definitions also reject nonempty context-only policy. Definition field
validation runs before a renderer for that selected YAML source. `description`
and `prompt` remain supported on modes. Metadata and tool descriptions are never
rendered, even when they contain template delimiters.

Absent new JSON fields serialize away; an empty native definition retains its
preceding bytes. Public tool descriptions and input schemas are unchanged when
no selected override exists. Explicit output policy changes only the selected
Serena transport/discovery additions. All source, write, runtime, approval,
deadline, cancellation and recovery authorities remain separate. The historical
53-tool contract JSON and original 353-entry ledger are unchanged.

Two inert declaration-data tests observed RED before implementation, then pass.
They check the compiled public field contract, not native execution. Four native
unit tests are authored for absence/null serialization, field scopes/types and
budgets, mode refusal and complete structured-result capacity. Six integration
tests are authored for full context/mode YAML and null prompts, rejection before
rendering, description/exclusion preservation, actual stdio output/error parity,
fixed startup/stale binding and actual authenticated HTTP output/source scope.
Unicode, combining characters and CRLF fixtures must remain byte-exact.

Native compilation and these ten tests are **unrun locally**. Exact hosted
qualification remains required. This does not certify complete global YAML,
all client/SDK behavior, backend runtimes, broader current upstream parity,
platform/VSIX/main/release or visual acceptance.
