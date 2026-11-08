# Guarded JSON MCP host profiles

The [checkpoint](checkpoint.json) records 175 scoped Rust tests passed and one
explicit child worker ignored, plus format, all-target Clippy with warnings
denied, typecheck and TypeScript 260 passed / 3 existing skips. The final guard
uses frozen inputs. Only recovery test code differs from the preceding green
run. These overlapping runs are not added together. Documentation follows checks.

Actual Claude Code 2.1.306 reads the managed `.mcp.json` from an owned temporary
working directory, with only this server approved in isolated client settings.
Both `mcp get` and `mcp list` report Connected. Exact generated entry import into
private user scope also connects. Native install/uninstall restores original
configuration and source bytes. All commands in these two connected live gates start suspended, enter their own
Windows job before resuming, and leave verified empty jobs after cleanup. No
login or model turn is requested; real user configuration is not selected.

Three new recovery functions each cover Cursor, Claude Code and Claude Desktop:
actual partial-population exit/resume/uninstall, prepared forced kill/rollback
of the same original file object, and replaced-source refusals with no writes.
The existing generic/Codex recovery, ownership and security tests remain passing.

Raw development failures remain: patch-r1 missed a Rust Context import;
guard-r1 exposed a hardcoded legacy profile whitelist; client-live-r1 had a
Python helper constant error before child creation; client-live-r2 shows status
subcommands ignore global --mcp-config; client-file-live-r1 refused a target
held as the installer's working directory. The corrected tests run from outside
targets. That Windows sharing refusal remains a documented boundary.

The [index](index.json) hashes all archived bytes except itself. Controllers are
their final versions; earlier phase errors are retained in their raw results.
Client binaries are not vendored. Their public manifest and executable hashes
are retained. The client health gate is not a model tool-call or interactive
--mcp-config session test. Cursor/Desktop runtime, hooks/statuslines, Unix and
broader R1-R9/Windows/current-head hosted/upstream acceptance remain open.
