# Explicit authored memories in live prompts

The opt-in live Session renderer supports `embed_memory(name)` for an explicitly
selected set of existing application notes. These are Code Intelligence authored
notes, separate from central AgentMemory. No note directories, files, user
memories or default catalogs are searched automatically.

## Host selection

Example host profile, with command wrapping shown separately:

```json
{
  "context": "desktop-app",
  "modes": ["editing"],
  "promptTemplates": {
    "context": {
      "templates": {
        "system_prompt": "{{ embed_memory('guide') }}"
      }
    },
    "systemPrompt": "system_prompt",
    "memoryEmbeddings": ["guide"]
  },
  "steering": {}
}
```

```text
yoke --root C:/owned/project --allow-session-templates serve-session
  --profile serena --id owned --host-profile C:/host/profile.json
```

The same independent live-template host grant and typed renderer capability are
required. JSON cannot supply a callback or select a worker. The function accepts
one positional string name; it can access only the host's selected names. The
function is registered only for this explicit route. An absent selection retains
the preceding config fingerprint, default behavior and worker protocols 1/2/3.
An explicit empty list enables an empty selection, not automatic discovery.

A bare name selects the active project's own note namespace. The exact `global/`
prefix selects only the host-registered `globalNotesRoot`, which must already
contain initialized application state. Names preserve spelling and Unicode; they
are virtual note addresses, never filesystem paths. Existing path/protected-name
checks, duplicate refusal, session state, no-memories mode, raw read_memory tool
policy, mem.read_memory API policy and effective ignored patterns remain enforced.
Read-only notes remain readable. Admission checks all selected names before
opening either payload store; a missing/uninitialized registered store refuses.

Each rendering reads current immutable note revisions through existing-only
read-only Store access. It creates no schema, source capture or note revision.
SQLite can coordinate existing WAL through sidecars. A successful Session/HTTP
host admission may independently create ordinary Session state before rendering;
no claim of a write-free complete command or absent primary Store is made.

## Output and honest provenance

An available note produces this plain-text formatting:

```text
<memory name="guide">
original authored note content
</memory>
```

The name is escaped for its attribute. The body preserves the original content
and hash, including Unicode and CRLF. It is output data and is never parsed as
another Jinja template. These tags are formatting; note text cannot grant host
registration, execution, approval, source-write, network or model authority.

Receipts bind exact revision, project/global scope, storage/source workspace,
original source snapshot, content hash and original anchors. They explicitly
label authored claims, sourceBound=false and anchor currentness not revalidated
for prompt embedding. An original anchor is not represented as a current source
fact. Unused/raw bodies are excluded from binding/usage receipts. Actual used
names, bounded call counts and per-fragment decoded/rendered hashes are retained.

Missing/deleted selected notes render no body and add a named_memory_unavailable
warning. Per-fragment partial=true and aggregate memoryEmbeddingsPartial=true
make the omission explicit. An unselected name, hidden note, disabled mode,
excluded read operation, malformed store/revision, stale Session or changed note
head refuses the response. Failed initialization stays uninitialized and returns
no partial instructions. All selected note observations and current Session state
are rechecked before delivery. Reads across stores are separate observations,
not an OS-atomic snapshot against equal-account actors.

Mode/project changes affect the next rendering. The selection and helper
contract version bind the Session fingerprint. Library `RenderInput.memories`
is an optional immutable resolved snapshot; custom `Runtime::trusted` callbacks
remain explicit host authority and own their own execution/resource guarantees.
The shipped CLI always uses the fixed native memory-bounded renderer.

## Deferred policy and discovery

A deferred `session-config` selection can additionally contain:

```json
{
  "memoryEmbeddings": ["guide", "global/team"]
}
```

This field belongs alongside templateContext/systemPrompt in the closed policy
selection, not in YAML authority fields. Plan/export/discovery still require
--allow-configuration-read, --allow-policy-projection, --allow-template-evaluation
and --defer-template-evaluation. The resulting promptTemplates carries the selected
names as data. It does not load notes or create a renderer. Discovery preserves
the selection for replay; starting an exported profile still requires independent
--allow-session-templates authority. Eager projection rejects memory selection.

## Bounds and remaining acceptance

There are at most 32 distinct names, at most 500 bytes per underlying note name
plus the exact global/ prefix, and at most 256000 serialized resolved-memory
bytes. Existing 64000-byte note content and 100-anchor bounds remain. SQLite's
connection-local allocation ceiling precedes reading stored JSON and is restored
after success or failure; malformed/oversized data is never empty success.
Metadata receipts are separately bounded and omit original bodies.

Protocol 4 carries only resolved inert note data and an exact snapshot hash in
its runtime binding. Protocols 1/2/3 reject memoryContext, including null. Every
fragment retains the 8192-byte output, 128 helper-call, 100000 fuel, recursion64,
inherited/10s deadline and checked 256MiB parent/self memory limits. Combined
instructions retain their existing 131072-byte ceiling. These are interpreter
and resource controls; no OS filesystem/network sandbox is claimed.

Default/upstream prompts, automatic memory selection, additional Python-Jinja
calling/conformance behavior, layouts/extensions/clients and broader upstream
parity remain open. Windows-owned qualification does not certify Unix, current
whole Windows, hosted CI, canonical VSIX, Google, main integration or release.
See [source-bound evidence](../../docs/validation/integration-2026-10-23/prompt-memories/README.md)
and the [live prompt contract](session-prompt-runtime.md).