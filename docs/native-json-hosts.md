# Managed JSON MCP clients (native 0.5.0 development)

The explicit JSON MCP profiles manage one named stdio server under `mcpServers`.
They use the guarded Windows installation, receipts and recovery described in
[host installation](../native/docs/host-install.md). Selection and planning do
not discover a home directory, invoke a client/model, execute project code,
index source or grant writes. Generic and Codex v1 identities stay unchanged.

| Selected client | Profile | File in each selected target root |
| --- | --- | --- |
| `cursor` | `cursor-mcp-v1` | `mcp.json` |
| `claude_code_explicit` | `claude-explicit-mcp-v1` | `.mcp.json` |
| `claude_desktop` | `claude-desktop-mcp-v1` | `claude_desktop_config.json` |

All directories and launch files are existing, absolute host selections.
The configuration, source and private-state directories must be distinct and
nonnested. For Cursor select the desired external `.cursor` directory; for
Claude Desktop select its external `Claude` configuration directory. No client
restart is performed. The Claude Code profile selects an external configuration
file for `claude --mcp-config <absolute-file> --strict-mcp-config`; it does not
manage a `.mcp.json` inside analyzed source or the user's `.claude.json`.

This documentation-only example must be replaced by real host-selected paths:

```json
{
  "installation": "json-yoke",
  "targetRoots": ["C:\\HostConfig\\Claude"],
  "executable": "C:\\Tools\\Yoke\\yoke.exe",
  "policy": "C:\\HostPolicies\\yoke.json",
  "stateRoot": "C:\\HostState\\Yoke",
  "adapter": {
    "kind": "json_mcp_v1",
    "client": "claude_code_explicit",
    "sourceRoot": "C:\\Projects\\example"
  }
}
```

Run `yoke host-install plan --selection host.json`, review and save the exact
returned plan bytes, then separately apply with `yoke host-install apply
--selection host.json --plan reviewed.json --allow-host-writes`. The write flag
is required before selection/plan reads. Fields in JSON never grant writes.
Status, uninstall-plan and explicit resume/rollback/initialization recovery use
the same selected profile and physical bindings. Run the installer from outside
the selected configuration directories; a process holding one as its working
directory can cause the guarded Windows namespace open to refuse sharing.

The owned name is `yoke_json-yoke` for this label. Its entry contains the selected
executable, fixed argv `--root <source> serve-session --id <label>
--host-profile <policy> --profile native`, and an empty `env` object. No shell,
client approval defaults, hooks, models or nonstandard working-directory fields
are emitted. An empty `env` is not an operating-system environment sandbox.
Native host permissions remain independent of client permissions. Select a valid
native HostProfile, such as the read-only example in the
[Codex workflow](native-codex-host.md), and explicitly index saved source before
queries. Connecting the configured client starts the selected native server.

Clients can interpolate `${...}` in configuration values. These profiles refuse
that sequence in selected source/executable/policy paths so generated launch
values remain literal. Physical directory/file identities and executable/policy
hashes are checked before planning, application and recovery. Unknown clients,
fields, profile substitutions, stale bindings and imported write grants fail.

Configuration is strict unique-key UTF-8 JSON, with an optional BOM, bounded to
1 MiB. Comments and trailing commas are refused. The independently exposed pure
`json_mcp_patch` API admits a tight object body up to 64 KiB and a bounded ASCII
server name. Installation creates a missing file or `mcpServers` object as needed.
An existing entry is never adopted, even if its contents match. Receipts own the
exact member bytes and separately record whether document/container shells were
created. Uninstall removes only those bytes and one necessary comma; unrelated
values, Unicode, BOMs, whitespace and line endings stay authoritative. Created
shells are removed only while their exact owned wrapping bytes remain unchanged.
Human formatting added outside the entry is retained. An original empty file
remains present; only an installer-created file can be removed as empty.

Private journals retain owned fragments and identities, not full human config
or policy files. A changed owned entry requires reconciliation. Recovery preserves
foreign occupants and stale bindings. Multi-file visibility is not atomic, and
power-loss durability and non-Windows installation are not established.

Windows fixtures cover install/uninstall, existing-empty versus absent files,
foreign growth, duplicate/escaped keys, mirrored entry bytes outside the selected
path, Unicode/CRLF/BOM, stale executable and source bindings, interpolation,
protocol/budget/deadline refusals and independent CLI write gates. Actual child
processes stop during partial population or are killed after preparation for each
profile, then exercise resume/uninstall and rollback to the original file identity.

Actual **Claude Code 2.1.306** validation loads the managed `.mcp.json` from an
owned temporary working directory with only this server approved in isolated
client settings. `mcp get/list` report a connected stdio server. A separate private
user-scope import also accepts the exact generated entry and connects. No login
or model turn is requested. Every command in the two connected live gates starts suspended,
enters its own Windows job before resuming, and leaves the job verified empty
after cleanup. Original configuration and source bytes roundtrip unchanged.

The tested `mcp get/list` commands ignore global `--mcp-config`; they are not an
explicit-file interactive-session test. Loading that flag in a real session,
Claude tool dispatch/model selection, Cursor and Claude Desktop runtimes, other
client releases, hooks/statuslines and wider platform/upstream acceptance remain
open. Schema compatibility alone does not certify those clients.

Primary configuration references: [Claude Code MCP](https://code.claude.com/docs/en/mcp),
[Cursor MCP](https://prod.cursor.com/docs/mcp), and the
[MCP local-server guide](https://modelcontextprotocol.io/docs/2026-07-28/develop/connect-local-servers).
