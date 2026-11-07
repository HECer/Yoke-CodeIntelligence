# Host-confirmed required authored notes

An explicit `requiredPromptMemories` selection makes selected existing application
notes mandatory for Session startup and instruction delivery. A normal `rule`
label or an optional `embed_memory` call does not create this selection. This is
an independent MIT implementation; the current Trail
[public consent contract](https://github.com/trailhq/Trail-Code/blob/a44da3dfb5894c8e316e320526041ef8e5ddbb8c/src/notes/always-follow-gate.ts)
is reference data, not imported application code or authority.

This candidate has authored native tests and source checks only. Native typing,
compiler tests and actual CLI/stdio/HTTP execution remain unqualified until the
guarded hosted run completes. It does not establish full Trail or R1–R9 parity.

## Explicit selection and confirmation

First use the ordinary explicitly selected
[memory embedding](prompt-memories.md) inspection to obtain a real available
note's revision receipt. The native `initialInstructions` result exposes it at
`promptRendering.binding.memorySnapshot.entries.<name>.revision`. That receipt
contains identifiers, hashes and authored provenance; it is not proof of a
person's identity or current source truth. Do not invent identifiers.

The private host profile can then select exact observed pins:

```json
{
  "promptTemplates": {
    "context": {"templates": {"root": "Review source before changing it."}},
    "systemPrompt": "root",
    "memoryEmbeddings": ["guide"]
  },
  "requiredPromptMemories": {
    "global": [],
    "projects": {
      "default": [{
        "name": "guide",
        "revisionId": "<observed revisionId>",
        "contentSha256": "<observed contentSha256>",
        "storageWorkspaceId": "<observed storageWorkspaceId>",
        "sourceWorkspaceId": "<observed sourceWorkspaceId>",
        "snapshotId": "<observed snapshotId>"
      }]
    }
  }
}
```

Each placeholder must be replaced with its actual 64-character lowercase hex
value. Every required name must already belong to `memoryEmbeddings`; no note
discovery or extra read permission is introduced. Global pins require exact
`global/` addresses and an initialized host-registered global note Store.
Project pins use bare addresses and select `default` or an existing registered
project. Equal names in different projects remain separate namespaces and need
their own exact workspace/revision pins. Global notes precede project notes.

JSON declares the policy but cannot confirm it. The shipped CLI requires both
independent host switches:

```text
yoke --root C:/owned/project --allow-session-templates \
  --allow-required-prompt-memories serve-session --profile serena \
  --host-profile C:/host/profile.json
```

The same switches apply to native Session batches, HTTP Session principals and
the separately authorized persistent REPL. HTTP admits every principal's
declarations and switches before resolving credentials. Stateless native HTTP
targets cannot use this Session feature. Existing service/REPL/runtime grants
remain independent.

Library hosts must independently supply the existing trusted prompt renderer
and call `required_prompt_memories::Runtime::confirm` after selecting the policy.
The returned capability has no serialization or default constructor and binds
the exact policy and prompt configuration. `revoke()` invalidates all its clones,
including existing Session handles. A new explicit confirmation can authorize a
new lifecycle; there is no automatic reconfirmation after a note changes.
No transcript keywords, attribution label, note body or model request constructs
this capability. Confirmation records explicit host selection, not authenticated
human identity, physical repository ownership or semantic truth.

## Lifecycle and delivery

Required notes use the existing current mode/tool/API/ignored-note admission and
existing-only read-only Stores. No schema or note revision is created while
resolving them. Each pin must match the available note's exact revision, content
hash, storage/source workspaces and original snapshot. Missing, deleted,
malformed or changed required notes refuse the entire startup/instruction result;
they never become an optional partial warning. Optional embeddings retain their
existing partial-warning behavior.

Startup checks precede new backend admission. Session metadata and independently
authorized activation-hook effects may already exist at that point; failure is
not a rollback of those operations. Project activation checks the next project's
note selection before its hook, again before engine admission and before Session
state publication. Selected mode/state transitions also check their prospective
read policy. A transition cannot silently disable required reads. New project
engine access checks that project's selected pins, including cached engines.

After a project/mode transition, its successful configuration response includes
fresh `instructions` and `promptRendering` when required notes are selected.
If later rendering fails, the error states that Session selection may already
have changed; callers can inspect state and must not assume rollback or retry.
Passive configuration inspection does not itself render or load note bodies.

Each instruction result appends the selected required bodies once, outside
template evaluation. Original Unicode, CRLF and template-looking text remain
literal bytes. A host template may independently request optional embeddings;
those explicit calls are not deduplicated or hidden. Formatting tags are not a
security boundary. Authored content remains subject to the consumer's normal
instruction hierarchy and grants no source, execution, write, approval, model or
network authority. The nonfallible library `instructions()` helper remains raw
literal steering; dynamic transport delivery uses `prompt_instructions()`.

Receipts contain selected pins and host-confirmation provenance, not note bodies.
They explicitly retain `humanIdentityAttested:false`, `sourceAuthority:false`
and `overridesInstructionHierarchy:false`. Original note anchors do not become
current source facts. Before delivery, Session state, note observations and live
confirmation are rechecked. These are separate checked observations, not an
OS-atomic guarantee against another process with the same account privileges.

## Bounds and compatibility

There are at most 32 pins in the entire policy and 101 project selectors. Policy
and receipt encodings each fit 64,000 bytes; original note/snapshot bounds remain.
The complete instruction result, including required bodies, must fit the existing
131,072-byte bound and the transport's own output bound. Overflow refuses the
whole result. All work retains the inherited deadline; no budget is renewed.

An absent policy omits the new serialized fields and preserves preceding Session
fingerprints, default behavior, optional partial semantics and worker protocols.
Selected policy changes bind the Session fingerprint; an existing named Session
cannot silently adopt different pins. Deferred configuration projection strips
both renderer and required-note confirmation capabilities from exported data.
No upstream application, model or analyzed project code runs by default.
