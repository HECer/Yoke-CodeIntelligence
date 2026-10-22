# Explicit YAML session configuration

The session-config command projects explicitly selected global, project,
project-local, custom-context and custom-mode YAML data into the existing native
HostProfile. No files are discovered automatically. The host supplies its base
profile and exact absolute paths/original byte SHA256 pins in a closed selection:

    {
      "hostProfile": {"context": "desktop-app", "steering": {}},
      "files": [
        {
          "path": "C:/host-policy/global.yml",
          "sha256": "<original byte SHA256>",
          "target": {"kind": "global"}
        },
        {
          "path": "C:/host-policy/project.yml",
          "sha256": "<original byte SHA256>",
          "target": {"kind": "project", "project": "default"}
        }
      ]
    }

Other targets are project_local with a project selector, and context/mode with a
custom name. Project selectors must already exist in the base host profile;
default selects the primary project. Custom files cannot overwrite built-ins.
The base retains CLI-layer settings and all host project/runtime/permission
registrations. No YAML field can create those grants.

Commands and flags below are shown wrapped. Plan returns the resulting hostProfile
and ordered source receipts without a Store or output. Export also creates a
fresh JSON profile:

    yoke session-config plan --selection /absolute/selection.json
      --allow-configuration-read --allow-policy-projection

    yoke session-config export --selection /absolute/selection.json
      --output /absolute/new-profile.json
      --allow-configuration-read --allow-policy-projection --allow-output

All required grants precede selection input. Source/provider/global-write flags
are unavailable. Input files and ancestor paths reject links/reparse points,
device/network spellings and hardlinks. Windows file/directory handles exclude
writers/replacements during the operation. Every input, including the selection,
is rechecked before reply/publication. Export admits reply capacity before writing
and preserves existing files/directories/links. A delivery failure after publication
reports the committed profile hash.

Select the exported JSON with the existing session/serve-session/agent-repl
host-profile argument. Equivalent JSON and YAML projections retain the same native
fingerprint and resume the same session binding. Changing YAML does nothing until
explicit reimport; changed policies retain ordinary stale-session refusal.
Original YAML bytes are never rewritten.

## Fields and boundaries

All roles support excluded_tools, included_optional_tools, fixed_tools,
excluded_apis, included_apis, read_only_memory_patterns and ignored_memory_patterns.
Global/project/project-local also support agent_interface, default_modes and
added_modes. Global base_modes supplies existing always-active modes.
Project/project-local initial_prompt and custom-definition prompt become literal
instructions. A selected nonempty source replaces its corresponding layer or
definition; other layers remain intact. Empty global/project mappings leave the
profile unchanged. Existing exclusion/inclusion/fixed conflicts and native
name/regex validation apply after all selected files are decoded.

Project read_only only tightens an existing restriction: named targets constrain
registered HostProject values, while default constrains primaryReadOnly. False
cannot clear a true base or earlier restriction. project_name remains an
unverified display declaration and never renames/registers a project.

Prompts preserve decoded Unicode/YAML block and folded-scalar semantics; receipts
bind original BOM/CRLF bytes. Jinja expression/statement/comment syntax refuses.
No template, memory, environment or shell evaluation occurs. Other keys, including
activation commands and server/runtime/trust/registration settings, refuse
explicitly. This is a native steering projection, not full upstream YAML/Jinja or
all 25 project-setting equivalence.

The strict event parser rejects aliases, anchors, tags, merge/duplicate keys,
complex/non-string keys, multiple documents, float/non-finite values, malformed
UTF-8/NUL and invalid types. Limits are 268 files, 65536 bytes/file, 2000000 aggregate
source bytes, 8192 parser events/file, depth 16 and 128000 serialized profile bytes.
Selection JSON is at most 128000 bytes. Existing per-instruction/name/layer limits
and shared deadlines apply; reply capacity is separate. OS reads are not a hard
time/memory sandbox.

The importer opens no project Store, interpreter, provider, network endpoint or
model. Receipts carry original sizes/hashes, target and display declaration, never
execution authority. Unsupported capabilities are errors rather than empty complete
results. Automatic discovery, broader project/server/client settings, dashboard
and Jinja remain separate work.

Public field names were checked against the
[pinned configuration guide](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/docs/02-usage/050_configuration.md).
No upstream application code or prompt text is copied.

## Primary-project read-only policy

HostProfile.primaryReadOnly is a startup boolean. Absent or false retains preceding
serialized profiles, host fingerprints, session bindings and replies. True binds
an additional versioned fingerprint and exposes primaryReadOnly:true in current
configuration. Switching modes/projects or passing tool arguments cannot clear it;
changing the host selection requires a different session binding.

True denies source edit/refactor preparation through native session, MCP and REPL
callbacks. Semantic query carriers that prepare plans require the same editing
permission before invoking a server. The primary Engine source-application grant
is also clamped, so an already approved and verified workspace plan cannot apply.
Source reads, immutable observations and independent note policies remain available.
This application policy does not sandbox a separately trusted arbitrary Python
or shell process or alter filesystem permissions.

The original global host write grant is retained separately for other registered
projects. An independent writable secondary can still prepare, approve, execute
verification and apply a change when all existing gates pass; named read_only and
query_only restrictions remain intact. Writable registrations that overlap the
primary root, including canonical aliases, ancestors and descendants, refuse
before session creation. Read-only/query-only overlapping registrations are allowed.

Selected project/project_local YAML read_only for project default now OR-tightens
primaryReadOnly. False cannot clear a true base or earlier restriction. Named
project behavior remains restrictive. YAML still cannot supply registrations,
runtime trust, source-write rights or automatic execution.
