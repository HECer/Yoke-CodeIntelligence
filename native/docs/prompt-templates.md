# Explicit native prompt templates

The separately granted [live Session route](session-prompt-runtime.md) now
derives available_tools, available_markers and tool_names. Deferred YAML exports
retain decoded scalars for that route. Eager evaluation below keeps literal
exports and protocol-1/2 compatibility. embed_memory and full Python-Jinja
equivalence remain unavailable.

Optional session-config plan, export and discover can render decoded prompt
scalars before producing the ordinary host profile. Select templateContext in
host selection JSON and pass --allow-template-evaluation in addition to the
existing read/projection grants. Export also requires --allow-output.
Add the following property to the existing selection object (other fields omitted):

```json
{
  "templateContext": {
    "variables": {"subject": "owned", "rules": ["Read source", "Keep evidence"]},
    "templates": {"heading": "Evidence for {{ subject }}"}
  }
}
```

For example, a selected custom mode file may contain:

```yaml
prompt: |
  {% include 'heading' %}
  {% for rule in rules %}- {{ rule }}
  {% endfor %}
```

Only context/mode prompt and project/project-local initial_prompt are rendered.
YAML is parsed and its supported settings checked before rendering. Policies,
paths, target names, tool/API grants, registrations and runtime trust cannot be
created by template output. The result is one plain instruction string, subject
to native steering and serialized-profile bounds. Exports contain literal data;
sessions do not reevaluate templates or fetch new variable values. Select the
resulting mode/context through the existing host mechanisms.

Without templateContext, existing plan/export/discovery replies and literal
profiles retain their preceding bytes. Template markers still refuse by default.
A context without the separate host grant refuses before selected YAML access;
a grant without a context also refuses. Discovery carries the context into its
pinned selection, but replay still requires the independent template grant.
No default/home/project-directory search is added.

MiniJinja 3.0.3 is linked with builtins, macros, multi-template support, loop controls,
Unicode and fuel; JSON inputs are converted only to inert data values. Conditions,
loops, filters, macros, includes, imports and inheritance work over explicitly
registered catalog strings. There is no filesystem loader, environment/memory
lookup, Python object/function registration, shell, provider, model or network
operation. Missing values are strict errors; missing catalogs refuse unless the
template itself explicitly uses the language's ignore-missing behavior. HTML
autoescaping is disabled because the output is instruction text, not rendered HTML.
Original source bytes remain hash-bound, including BOM/CRLF/Unicode. Receipts bind
the decoded original prompt, rendered UTF-8 and canonical selected context separately.

For the preceding inline protocol 1 route, the context is at most 128000
serialized bytes within the existing 128000-byte selection. Its inline templates
mapping contains at most 32 names, each at most 65536 bytes; bounded relative
catalog names are opaque identifiers, not file paths. Every render has
100000 fuel, recursion 64, at most 8192 output bytes and a parent-controlled 10s
capture intersected with the caller's deadline and cancellation. The request is
at most 256000 bytes. The normal parent sets a 256MiB OS memory ceiling before
execution, and the dedicated worker independently applies/observes its own ceiling
before reading input. Windows limits committed job memory; Unix limits virtual
address space and fails if it cannot enforce the requested limit. Unix execution
is not qualified by the Windows evidence. Existing checked termination and member
proofs remain; a failure does not publish a partial profile or create an export.
OS resource limits do not provide an account/filesystem/network sandbox.

The library's separately explicit project_with_prompt_renderer hook accepts a
trusted host callback; the caller owns its authorization, limits and external
evidence. The ordinary project API never calls it. CLI selection cannot register
a callback: it always uses the fixed native worker. Host-renderer labels are
supplied evidence, not independent semantic attestation.

The 13 integration functions and three added library units are included in the
722-pass/seven-existing-ignore qualification. Actual literal/rendered CLI profiles,
raw session/MCP replies, returned instructions and original SQL/source rows agree.
A direct 1GiB indent allocation fails inside the bounded worker without partial
stdout. See [exact source-bound evidence](../../docs/validation/integration-2026-10-23/prompt-templates/README.md).

MiniJinja is Apache-2.0; the independent own core remains MIT. See its
[pinned package manifest](https://github.com/mitsuhiko/minijinja/blob/3.0.3/minijinja/Cargo.toml)
and [engine API](https://docs.rs/minijinja/3.0.3/minijinja/struct.Environment.html).
The external prompt-in-YAML surface is documented in the
[pinned configuration guide](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/docs/02-usage/050_configuration.md#custom-prompts).
No upstream application code or default prompt text is copied.

Automatically selected/default prompt catalogs, automatically derived
runtime/tool variables, upstream-specific helper filters, exact Python-Jinja
conformance, broader clients and full upstream/R1-R9 acceptance remain separate.
The result keeps complete=false and confers no execution/write authority.

Explicit pinned YAML catalogs, flat host-selected prompt_templates discovery,
collision provenance and named create-only text output are now supported by the
[separate catalog contract](prompt-catalogs.md). Their pure-data protocol 2 extends
only resolved catalog/named budgets. The preceding inline protocol 1 and all of its
original limits, raw replies and default literal behavior remain unchanged.
