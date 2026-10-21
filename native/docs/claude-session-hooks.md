# Explicit Claude SessionStart settings

`claude-session-hooks-v1` is an optional Windows host-install profile. It creates
one entirely owned `yoke-claude-hooks.json` in each explicitly selected existing
configuration directory. The caller chooses that file with Claude's `--settings`
option. Nothing discovers or changes home/project settings, patches an existing
hook array, approves tools, enables a plugin, or starts a client automatically.
An existing file, including an empty or byte-identical file, cannot be adopted.

The ordinary host selection JSON adds:

```json
"adapter": { "kind": "claude_session_hooks_v1", "sourceRoot": "C:/owned/source" }
```

It also requires `installation`, `targetRoots`, `executable`, `policy` and
`stateRoot`, as described in the host installer contract. Use `host-install plan`,
review the result, and explicitly grant `host-install apply --allow-host-writes`.
Source, configuration and journal roots must be separate and nonnested. Paths
must be literal absolute local paths; client `${...}` interpolation, aliases,
stale identities and obvious secret material refuse. The native executable and
host policy are bound to physical identity, length and hash. The launch also pins
the source directory and existing configuration/journal roots. The old generic,
Codex and JSON MCP profiles keep their original serialized identities.

The generated settings handle `SessionStart` for startup, resume, clear and
compact. They invoke a real native executable with an `args` vector, without a
shell, using `claude-session-start --launch-json ...`. There is no command string
quoting, transcript read, source capture, Store creation, Git/model/network call,
project code execution, permission decision or initial user message. The callback
emits one advisory `additionalContext` object identifying the bound source scope;
`snapshotStatus` is explicitly `not_checked`, and permissions/execution evidence
are false. This does not configure an MCP server or prove available source tools.

Hook stdin is untrusted data. Unique-key JSON, 64 KiB, depth and output budgets
are enforced; only the closed event/source fields and an exact literal match of
`cwd` to the selected source path are consumed. A transcript path and future
fields never select files, commands or authority and are not echoed. A client
working in a different directory, including a source subdirectory or path alias,
refuses. The short-lived CLI has a deadline for a stalled stdin producer and
returns a static refusal marker without partial JSON output.

Every callback checks the original physical bindings and a current installed
receipt with no pending recovery and unchanged managed files. A changed policy,
executable, source object or settings refuses. Installation status and receipt
inspection are read-only. Normal host-install uninstall and interruption recovery
apply to this wholly owned file; foreign settings remain untouched. Callback
failure is advisory and is **not** an enforcement hook or a security boundary
against a hostile client/host.

Claude Code 2.1.306 supports literal exec-form hooks and the conversation-free
`--init-only` path in an isolated owned fixture. See the official
[hook reference](https://code.claude.com/docs/en/hooks). Actual managed native
callback validation is recorded separately; client parsing alone is not callback
delivery or context consumption by a model. Interactive/model behavior, other
client versions, statuslines, Linux/macOS and full R6 acceptance remain separate
gates. No login or paid model call is needed for the init-only fixture.
