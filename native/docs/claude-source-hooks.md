# Explicit source hooks and status output

The Windows `claude-source-hooks-v1` profile manages the entire separately owned
`yoke-claude-source-hooks.json` settings file. Select that file explicitly in
Claude Code. It does not modify user settings or adopt/merge existing files,
including an existing empty file. The preceding `claude-session-hooks-v1` profile,
filename and launch encoding remain separate.

Use the [managed installation](host-install.md) plan, review, apply, status,
uninstall and recovery workflow. Example host selection (all paths absolute):

```json
{
  "installation": "selected-project-source-hooks",
  "targetRoots": ["C:/host-owned/client-config"],
  "executable": "C:/host-owned/yoke.exe",
  "policy": "C:/host-owned/policy.json",
  "stateRoot": "C:/host-owned/install-state",
  "adapter": {
    "kind": "claude_source_hooks_v1",
    "sourceRoot": "C:/selected/project",
    "viewId": "<existing-source-only-View-ID>",
    "limits": {
      "maxTerms": 4,
      "maxSourceBytes": 4000,
      "depth": 2,
      "maxImpactVisited": 300
    },
    "statusShell": "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
  }
}
```

Build the source View explicitly beforehand with the native `index` and
`graph build` commands. Host-install planning and installation status do not open a source Store
or validate the selected View. Each actual callback requires an existing Store,
a valid immutable source-only View, the current installation receipt and no
pending journal. Selecting an ID does not register a new retention pin; keep the
required View through the separate explicit retention workflow. A missing
View/capability produces refusal rather than an empty complete result. Meaning overlays are not admitted by this profile.

The settings register direct executable/argv handlers for `SessionStart`
(startup/resume/clear/compact), `UserPromptSubmit`, and `PostToolUse`
(Write/Edit/MultiEdit). These are advisory `onFailure: continue` callbacks, not
permission hooks. The generated callbacks have a 20-second native budget,
65,536-byte output limit and 25-second client timeout. The finite CLI reads at
most 65,537 input bytes and refuses an over-limit event; stalled stdin is bounded
by the selected native timeout. Duplicate JSON keys and invalid UTF-8 refuse.
Errors use a static diagnostic instead of reflecting input text.

Prompt retrieval splits at non-identifier characters, keeps distinct Unicode
alphanumeric/underscore terms of at least two characters and at most 128 UTF-8
bytes, and queries captured entity names by substring. The first `maxTerms`
eligible terms are selected; this is not a semantic prompt interpreter or ranked
whole-prompt retrieval. A late symbol can be omitted. Prompt input is limited to
32,000 UTF-8 bytes and obvious secret patterns refuse. Each evaluated term uses
the existing checked context query with at most two results. Source bytes share
one cumulative budget; fewer than 100 remaining bytes stop further queries.
Original source text, hashes, exact byte anchors, observations and evidence stay
intact. Eligible/evaluated/omitted counts and `complete: false` remain visible.
Zero term or source-byte limits remain zero, after validating the selected View.

Post-edit reports use the explicitly pinned *preexisting* captured View. The
selected absolute `tool_input.file_path` must normalize to an existing captured
path inside the bound source root. Traversal, external/protected/uncaptured paths
refuse. At most min(maxImpactVisited, 100) entities in that path seed the existing
incoming trace. Parallel observation IDs and evidence remain distinct. This
reports incoming dependency candidates, not a fresh edited graph or exhaustive
blast radius. The result retains the captured hash, omitted seed count,
`editEventIndependentlyVerified: false`, `liveSourceRead: false` and
`complete: false`. Host tool input/response is untrusted observation; an actual
client fixture independently checks its own file edit separately.

The callback checks physical source/configuration/state/executable/policy
bindings and the exact installed settings before the query, and rechecks current
installation state afterward. Input `cwd` must equal the selected literal source
path; `transcript_path`, claimed permissions and event text grant no authority.
No transcript is opened. The policy file is an exact stale-precondition binding;
the callback does not parse it into session-tool permissions. The explicit host
source/View selection defines this read workflow. The callback uses existing-only
read-only SQLite. A first
reader can create SQLite coordination files (`index.sqlite-shm` and an empty WAL);
this is separately measured and does not mean no filesystem metadata is created.
Original SQL, captured/source bytes, host settings and receipts are preserved.

Hook context is serialized as `hookSpecificOutput.additionalContext` with
`producer: yoke-native-source-hook-1`, original snapshot/workspace/View IDs,
`sourceFreshness: not_checked` and `contentAuthority: untrusted_captured_data`.
Captured excerpts are data, not instructions or execution proof. No indexing,
source application, model inference, package/model downloads or execution of
analyzed project code follows. No completeness, current-disk freshness, observed
runtime dispatch or new execution/write permission is claimed.

## Optional status shell

Without `statusShell`, no statusLine setting or shell execution is added.
Claude's documented statusLine uses a shell-form command rather than the hook
`args` form. This profile therefore requires an explicit PowerShell executable
outside the source root; it never discovers one through PATH or home settings.
The outer executable token must be an absolute ASCII path without spaces or
shell metacharacters, named powershell.exe or pwsh.exe. Source, configuration and
state paths can contain spaces, Unicode and apostrophes.

The selected executable's physical identity, hash and size are bound in the
profile and rechecked by native operations. The client/outer shell and selected
runtime remain trusted host dependencies: a native callback cannot attest the
shell before that shell has started. Changing the selected shell or native
executable requires a new reviewed installation. This is not OS isolation.

The generated command uses `-NoLogo -NoProfile -NonInteractive -EncodedCommand`.
Its UTF-16 program starts only the bound native callback with explicit
an inherited binary stdin handle, separate ProcessStartInfo stdout/stderr pipes
and Windows CRT argument quoting. Inheriting stdin avoids a managed text writer
adding its encoding preamble before the original event bytes. Caller-supplied
BOM prefixes still refuse; no JSON/source normalization is added. The native
callback bounds input waiting, and process exit/output draining have finite
waits; unfinished owned
process cleanup fails closed. No execution-policy, system privilege or Developer
Mode changes occur. The status text contains only native version, a View-ID
prefix, entity count and `freshness unchecked`; foreign text/control sequences
are not reflected. Querying status still validates the current receipt and View.

The tested PowerShell5 path and isolated Claude2.1.306 hook dispatch are finite
Windows evidence. A local Anthropic-compatible fixture supplies deterministic
protocol responses, not real model inference or model-quality/cost evidence.
Actual interactive Claude status rendering, other client versions/hosts,
background refresh, session learning/team services, broad upstream equivalence
and universal accessibility remain open. An owned text renderer's screenshots
qualify only the captured status text, not the interactive client interface.

See the dated [validation checkpoint](../../docs/validation/integration-2026-10-23/source-hooks/README.md).
The later [stdin transport correction](../../docs/validation/integration-2026-10-24/status-stdin-handles/README.md)
has actual inert PowerShell5 byte-transfer evidence; its current native CLI and
interactive client acceptance remain pending.
Provider contracts: [hooks](https://code.claude.com/docs/en/hooks),
[statusline](https://code.claude.com/docs/en/statusline),
[permissions](https://code.claude.com/docs/en/permissions) and
[streaming](https://platform.claude.com/docs/en/build-with-claude/streaming).
The implementation is independent native code; no upstream application code is
copied. Provider documentation and exact client builds can change independently.
