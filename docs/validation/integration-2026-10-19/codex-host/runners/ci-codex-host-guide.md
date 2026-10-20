# Managed Codex MCP installation (native 0.5.0 development)

The explicit `codex-mcp-v1` host-install profile manages one named stdio MCP
entry in an explicitly selected `config.toml`. It uses the existing guarded
Windows installation, receipt and recovery machinery. The original
`generic-agents-v1` profile and its serialized identities remain unchanged.
This profile does not discover a home directory, install hooks, invoke a model
or execute analyzed project code.

Select existing absolute local directories and host-owned files. For example,
these documentation-only paths must be replaced with actual host selections:

```json
{
  "installation": "codex-yoke",
  "targetRoots": ["C:\\Users\\Example\\.codex"],
  "executable": "C:\\Tools\\Yoke\\yoke.exe",
  "policy": "C:\\HostPolicies\\yoke.json",
  "stateRoot": "C:\\HostState\\Yoke",
  "adapter": {
    "kind": "codex_mcp_v1",
    "sourceRoot": "C:\\Projects\\example"
  }
}
```

The source, configuration and private-state directories must be distinct and
nonnested. This finite profile therefore selects separate Codex configuration
homes; it does not manage a project-local `.codex` beneath the source root.
Executable/policy bytes and identities are bound without executing them or
storing their contents in installation records. Original ordinary launch paths
are separately bound to their guarded physical objects. Source directory
identity, launch paths and authority files are rechecked before planning,
application and recovery. A replaced directory cannot redirect an old plan.

Prepare and review a plan with `yoke host-install plan --selection host.json`.
Save the returned exact plan bytes to a host-selected file. Apply with
`yoke host-install apply --selection host.json --plan reviewed.json
--allow-host-writes`. The write flag is required before selection/plan reads;
putting a grant in JSON does not authorize a write. Planning and status do not
create configuration, private state, a source Engine or a source Store.

The profile owns `[mcp_servers.yoke_codex-yoke]` for the example label. It emits
the separately selected executable, a fixed argv containing `--root <source>`,
`serve-session --id <label> --host-profile <policy> --profile native`, and the
explicit source working directory. It configures six tool-discovery names:
`ci_find`, `ci_read`, `ci_trace`, `ci_context`, `ci_retrieve` and `ci_graph`.
Additional environment-variable forwarding is empty. The client tool list does
not replace native host policy or give source, execution, network or model
authority. The host policy file must be a valid native `HostProfile`.

For an explicitly read-only source session, an example host policy is:

```json
{
  "projects": [{
    "name": "main",
    "root": "C:\\Projects\\example",
    "readOnly": true
  }],
  "modes": ["planning", "query-projects"],
  "notesPolicy": {"writable": false}
}
```

Prepare a saved source snapshot with `yoke --root <source> index` before queries.
Connecting the client starts the explicitly configured native server and its
source session; installation itself does not start or index it. Configuration
startup and tool timeouts are 30 and 120 seconds. Other models, clients, tools
and sibling configuration stay under their existing host settings.

`host-install status --selection host.json` reports the current receipt ID and
pending journals. `uninstall-plan` followed by separately authorized `apply`
removes only the receipt-owned table. Human comments, neighboring servers and
separate appended tables are preserved. A modified owned entry, unowned server
with the same decoded name, ambiguous TOML ownership or human keys appended
under the owned table require reconciliation. An original empty configuration
file remains present; an installer-created file is removed only if its owned
removal leaves it empty.

Unknown or interrupted writes retain their exact journal. Existing explicit
`recover --mode resume|rollback --allow-host-writes` and
`recover-initialization --allow-host-writes` use the selected profile's
canonical edit, current physical/security bindings and exact ownership proof.
No corrupt or foreign state is reset. The generic v1 history remains separate
from v2 output-ownership recovery. State hashes are consistency metadata, not
authentication of storage controlled by an adversary. Multi-file installation
is not atomically visible, and power-loss durability is not established.

Actual Windows validation uses **Codex CLI 0.163.0** with an owned temporary
`CODEX_HOME`. The real client parses command/argv/cwd/tools, starts the installed
MCP server and dispatches a source read through its app-server MCP client. Exact
Unicode/CRLF source bytes roundtrip, and uninstall preserves human tables. No
model turn is started; an owned loopback model endpoint receives no generation
request. Its process group is explicitly cleaned and checked empty. This does
not test model tool selection, interactive IDE behavior, other client releases,
hooks, non-Windows installation or broader upstream equivalence.

The concrete configuration follows [official MCP settings](https://learn.chatgpt.com/docs/extend/mcp)
and the [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference).
The live test uses the documented [app-server MCP client API](https://learn.chatgpt.com/docs/app-server).
