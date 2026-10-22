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

HostProfile.gitIgnore selects whether Git-ignore rules filter project analysis:

    {"global":false,"projects":{"second":{"project":true,"local":null}}}

Local overrides project, then global. Null inherits; absent/null/empty policy
objects preserve old serialization, captures and bindings. Configuration and
symbol-query wire bytes remain exact. The independent public search correction
treats empty glob defaults as no restriction, so legitimate ordinary matches
previously lost by that bug are now returned. Explicit true uses existing filtering. Explicit false adds authorized Git-ignored source to real
capture and symbol search, including selected symbol workspace folders. Ordinary
.ignore rules, hard exclusions, protected/secret/link rejection, host source
exclusions and all capture/deadline budgets remain enforced. Original bytes and
UTF-8 offsets remain authoritative. No source or ignore file is rewritten.

Global/project/project_local YAML ignore_all_files_in_gitignore accepts a strict
boolean or null. The explicit pinned YAML projection supplies this analysis
selection only; it grants no project registration, write or runtime authority.
Named projects resolve independently from the original host capture scope.
Nonempty policy binds session identity and selected calls check current state.

Ignored-at-capture facts remain bound to immutable typed source proofs. Included
Git-ignored files stay read-only. Original ignored members cannot change in
native derived/overlay/archive chains, even after other source changes. Current
bounded local ignore rules also reject prospective ignored workspace writes,
including plans prepared under older settings, before a journal is published.
Ordinary authorized edits, isolated verification and guarded recovery remain
separate. Pattern-search defaults follow the actual project analysis selection;
explicit request skipIgnored filters still use original ignore facts. Public
tool normalization preserves omission for this setting, including external
project queries and persistent-runtime adapters. Empty public search glob
defaults mean no glob restriction. Selected tool discovery exposes
the actual project default; project changes use existing catalog notifications.

The Rust host capture option includeGitIgnored is false/omitted by default and
records this analysis choice in capture identity. It cannot be combined with the
separate native read-only includeIgnoredDirs exception; that combination refuses
explicitly, so a read-only inclusion cannot silently confer index membership.
A worktree/external .git file or unsafe/oversized current ignore-policy file
refuses workspace writes under this selection rather than weakening policy.
This setting does not certify any runtime or bypass verification and approval.

HostProfile.languageServers selects ordered, already registered catalog backends:

    {"global":["angular","typescript"],"projects":{"second":{"local":["rust"]}}}

Global/project/project-local YAML language_servers projects the same policy.
Local overrides project, then global; null inherits. Lists require one to 32
unique current catalog IDs. Unknown IDs, unregistered projects and invalid types
refuse; startup additionally requires every selected ID to be host-registered.
Empty/null policy objects preserve preceding serialized profiles and bindings.
The storeless YAML plan explicitly reports that runtime registrations have not
been validated and that none came from YAML. Source bytes remain hash-pinned.

The host supplies named registrations separately. Existing language/command/args/
configuration server entries retain their preceding behavior. A backendId entry
registers a dormant catalog provider, with a reviewed absolute executable or
explicit existing numeric-loopback TCP endpoint. CLI registration still requires
--trust-servers. HTTP principals use their trusted host server entries. No ID
downloads, installs, starts or trusts an executable. Project YAML cannot supply
registrations, commands, trust, initialization options or source-execution grants.

For each requested file, the first selected backend with a matching declared
catalog extension is used; otherwise the first backend is the explicit fallback.
Actual capabilities are negotiated; extension matching does not certify a runtime
or compiler. A selected provider failure remains an error rather than trying a
different provider. Direct/staged/owned semantic calls share this selection before
passive checks, profile hashing and startup. Selected replies identify the backend
and fallback decision. Profile hashes also bind its declared backend ID.

The host registry and complete project selection bind session identity. Named
projects resolve independently; changed registrations/order/settings require a
new binding, and stale sessions refuse. Registrations are sealed after opening
a project session. Unselected dormant entries do not change legacy dispatch,
serialized defaults or existing bindings. Lifecycle refresh probes only assigned
selected backends and reports unassigned entries without claiming to restart
external peers. A directory move spanning multiple selected backends refuses
before provider startup or plan publication; multi-provider repair is separate.
File reads, immutable graphs/source IDs, host runtime/source/write permissions,
approval, verification and recovery retain their existing independent scopes.

HostProfile.symbolWorkspaceFolders selects a separate structural symbol-index
projection and actual LSP workspace roots. For example:

    {"global":["src"],"projects":{"second":{"project":["packages"],"local":["é"]}}}

Global/project/project-local YAML ls_workspace_folders uses the same policy.
Local overrides project, then global; null inherits. Paths are relative to the
registered project, with an optional leading ./, or the whole-root selector . .
One to 32 unique, portable, unprotected folders are required. Empty lists refuse;
empty/null policy objects preserve preceding profile bytes and session bindings.
Unregistered project selectors, outside paths, duplicates and unknown fields
refuse. Missing, ignored-only and file-valued roots refuse against the selected
immutable snapshot before a symbol query or provider launch.

Filtering precedes symbol paging, root match limits, body search and descendants.
Replies identify the selected symbolIndexScope, original snapshot and base graph
hash; captured sources and the original graph/IDs remain unchanged. This is a
selected index projection over the complete captured graph: parsing/capture of
other sources still occurs. No reduced indexing cost or complete compiler index
is asserted. Ordinary file listing/reading/search, explicit graph views, traces,
references and source-write permissions retain their independent scopes.

Semantic calls send the selected folders as real workspaceFolders inside the
owned immutable provider tree. The full captured tree, including root project
configuration, remains available there. Explicit direct semantic requests may
target other authorized captured files; selecting folders grants no source,
runtime or execution authority. Existing passive-profile and source-execution
checks still apply. Named projects resolve independently; activation cannot
borrow primary selections. Policy changes invalidate the existing session
binding, and stale sessions refuse before queries/providers. Additional external
workspace folders remain separate work.

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
complex/non-string keys, multiple documents, nonfinite values, floats outside symbol_info_budget, malformed
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

## Source line endings

HostProfile.sourceLineEndings explicitly selects source-write formatting:

    {"global":"crlf","projects":{"default":{"project":"lf","local":null}}}

The closed object supports global and projects; each project object supports
project and local. Selectors must be default or an existing host registration.
Values are lf, crlf or native. Missing/null values inherit local to project to
global. Native resolves to CRLF on Windows and LF elsewhere. The host session
binding includes selected values and resolved platform results. Changed settings
require a separate session label; they cannot silently change a saved binding.
Absent/null sourceLineEndings retains preceding profile bytes and fingerprints.

Explicit global/project/project_local YAML line_ending projects to the matching
slot. A null clears that selected slot and inherits the remaining settings.
Input order does not change layer precedence. Context/mode line_ending, invalid
types, unknown values and unregistered project selectors refuse. YAML bytes and
configuration output are never reformatted.

The central plan producer first applies edits at original UTF-8 byte offsets,
then converts CRLF, lone CR and LF in each selected after-file. It does not add a
final newline or change other Unicode/BOM bytes. Only files named by edits are
formatted; other captured files and original snapshots remain exact. This is
explicit textual formatting, including literal newline characters in strings;
it is not a semantic-equivalence guarantee.

Expansion must fit the existing four-million-byte file budget before conversion
and publication. Deadlines, path scope, expected hashes, test-file approval and
all existing limits still apply. The resulting hashes, source graph and diff
describe formatted bytes. Selected plans record concrete sourceLineEnding lf or
crlf under a distinct versioned identity. Checked replay validates that declared
convention against actual immutable after-bytes. No native value is saved in a
plan and no current host setting is needed to replay it.

Approval and executed verification authorize the exact stored result. Applying
a saved plan never reformats it. A workspace application whose after-bytes
conflict with the current selected convention refuses before journal/source
writes; prepare, review and verify a new plan. Historical isolated copies and
recovery preserve exact planned/original bytes, independently of current format
settings. Already completed idempotent receipts retain existing semantics.

Provider effects/diagnostics remain evidence for the provider effect before this
separate host formatting step. Formatted compiler results identify that scope
and require verification of the resulting plan. Captured source is not executed
by formatting. Named notes, configuration and application state are outside this
source policy. Mode/tool/API/write/runtime gates remain independent; a setting
does not grant editing, source application or process execution.

The public setting names and values were checked against the
[pinned project template](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/project.template.yml).
Only public configuration facts are used; no upstream implementation is copied.
This checkpoint does not certify other project settings, upstream clients,
non-Windows runtimes or full parity.

## Source exclusion groups

HostProfile.sourceExclusions selects additional source filters at startup:

    {"global":[["private/**","!private/keep.rs"]],
     "projects":{"default":{"project":[["generated/**"]],"local":[]}}}

Each inner list is an ordered pattern group. A path excluded by any group stays
excluded; a later group or project/local layer cannot reopen an earlier group's
restriction. Within one group, later negations apply, but a child cannot reopen
an excluded parent directory unless that parent is also reopened. Existing
host capture scopes and hard exclusions still apply. Source files, byte offsets
and Unicode path normalization forms are not rewritten.

Explicit global/project/project_local YAML ignored_paths supplies one additional
group. Null and empty lists add nothing and cannot clear base host restrictions.
Context/mode selection cannot change source filters. Unknown registrations,
incorrect types, malformed patterns and unsupported settings refuse. The native
host syntax uses arrays of groups so original host constraints stay independent
of imported configuration. Absent/null/entirely empty sourceExclusions preserves
preceding serialization and session/capture/plan identities.

New captures store the concrete gitignore_v1 groups and caseInsensitive flags
in their existing immutable capture-option proof. Host-profile patterns choose
case-insensitive matching on Windows and case-sensitive matching elsewhere;
historical replay uses the captured flags. Matching uses ignore 0.4.33 without
discovering or reading global Git configuration. Unlike permissive Git syntax,
unclosed character classes are errors. Capture omissions identify files excluded
by this policy. An explicitly selected empty corpus remains visible as that
selection; excluded reads fail rather than returning empty complete source.

The active policy admits at most 16 groups, 128 patterns, 512 UTF-8 bytes per
pattern and 32768 combined pattern bytes. NUL and embedded line breaks refuse.
Compilation and matching check the shared deadline; finite input limits do not
make synchronous regex compilation a hard OS scheduling or memory sandbox.

Current write/preparation restrictions also apply when an old snapshot or fully
approved/verified plan is selected. Windows existing-directory aliases are
checked through the same canonical path resolution as captured write scopes.
Historical reads, checked plan replay, isolated copies and recovery retain their
original selection and bytes. Named projects derive their settings from the
original host capture scope; primary-only filters do not leak into independent
secondary registrations.

These are per-project analysis selections and additional write restrictions,
not an OS access-control or source-retirement mechanism. Existing saved source
remains accessible through separately authorized historical reads. Selecting a
different host registration uses that registration's scope. Source filters
cannot grant writes, runtime trust, network access, project registration or
permission to bypass protected paths, ignored-source read-only rules, stale
preconditions, approval, executed verification or recovery journals.

Other project keys, including workspace-folder overrides,
runtime settings and activation commands, remain separate explicit boundaries.

## Optional symbol-information budgets

HostProfile.symbolInfoBudgets selects a soft time limit for supplemental LSP
hover information, independently for each registered project:

    {"globalMs":10000,"projects":{"default":{"projectMs":1000,"localMs":null}}}

Local overrides project, which overrides global. Null inherits the next slot.
The native representation is integral milliseconds from 0 through 600000.
Zero disables early stopping; the host request deadline/cancellation still applies.
Absent/null/entirely empty policies preserve prior serialization, session bindings,
source IDs and replies. Nonempty policies bind session identity and current state.
Modes and project activation cannot rewrite a host-selected policy; activation
uses that registered project's own overrides without leaking primary settings.

Explicit global/project/project_local YAML symbol_info_budget accepts finite
nonnegative seconds up to 600, including decimals. Positive values round upward
to milliseconds; a positive value below floating-point representability refuses
rather than becoming unlimited. Zero is unlimited and null clears its slot to
inherit. Numeric strings, negative/nonfinite values, invalid roles and unknown
registrations refuse. Other YAML floats remain unsupported.

For lsp.find_symbol, lsp.find_implementations and the qualified persistent
lsp.find_declaration bridge, the clock starts immediately before the first
optional hover request. Each later hover checks elapsed time before starting.
An in-flight request can exceed this soft budget; it remains bounded by the
existing host timeout/cancellation and process lifecycle. No provider error or
host cancellation is converted into a successful partial reply. Mandatory
search results, source anchors, body requests and kind filtering remain intact.
Body-bearing find_symbol still suppresses optional info as before.

Selected responses identify budgetMs, retrieved/omitted counts and earlyStopped
under symbolInfo. Omitted items carry infoOmitted with an explicit reason;
symbolInfo.complete describes only the requested supplemental information.
JB facade calls retain their preceding behavior and do not use this LSP policy.
The setting grants no server, interpreter, source-write or network authority.

The public setting name, zero/unlimited semantics and soft-stop behavior were
checked against the [pinned global template](https://github.com/oraios/serena/blob/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/serena_config.template.yml).
No upstream application code or prompt text is copied.
