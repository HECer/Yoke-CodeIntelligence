# Dashboard language-server selection

The later Code Intelligence 0.5.0 [733-pass scoped acceptance](../../docs/validation/integration-2026-10-24/dashboard-documents/README.md)
qualifies current CLI stdio/HTTP and actual native-browser provider order,
explicit disable, reset and stale-draft handling. It supersedes the earlier
[disk-limited checkpoint](../../docs/validation/integration-2026-10-24/dashboard-languages/README.md)
for this scope. Owned fixture dispatch does not certify external backend versions
or universal semantic quality.

The host independently opts into selection from already trusted registrations:

```json
{
  "languageServers": { "global": ["angular", "typescript"] },
  "dashboard": {
    "management": {
      "sessionControls": true,
      "languageServers": {
        "backendIds": ["angular", "typescript"],
        "legacyLanguages": ["rust"]
      }
    }
  }
}
```

The top-level selection supplies existing host defaults. The nested allowlist is
separate authority for dashboard changes. Every listed backend/language must be
currently host registered. Backend IDs are known, distinct and bounded to 32;
legacy choices are distinct and bounded to 100. Empty allowlist arrays confer no
selection of additional providers. Absent/null `management.languageServers`
preserves prior serialized host/state bytes and dashboard behavior. No command,
executable, endpoint, trust setting or arbitrary project root comes from HTTP.

Private management actions use the existing authenticated, same-origin envelope,
current project and session SHA. `selectLanguageServers` contains either
`selection: {"kind":"backends","ids":[...]}` or
`selection: {"kind":"legacy","languages":[...]}`. Backend IDs are nonempty,
ordered and allowlisted: first matching extension wins, then first fallback.
Legacy languages select a registered subset; an explicit empty subset disables
semantic providers, whose semantic requests refuse rather than returning an
empty complete result. `resetLanguageServers` restores that project's original
host selection and legacy registrations. These actions require current
`ci_session` policy and `sessionControls`; no new public MCP operation is added.

Selections update metadata on the original owning Session thread. A SQL state
comparison precedes an infallible, prevalidated in-memory projection. Overrides
are project local, persist on reopen, and are revalidated against the current
host grant and pinned registry. Their serialized map is bounded to 32000 bytes
and 101 projects. Together with the existing ASCII label, mode and project limits,
the conservative full State bound is 61859 bytes, below the 64000-byte reopen cap.
The original registry snapshot prevents a disabled primary selection leaking
into a newly opened secondary project. Registry changes refuse; they cannot
reuse an older grant. Host defaults and immutable source/evidence are retained.

Providers have a per-request lifecycle. Selection alone starts no provider,
restarts no external TCP peer and certifies no runtime or semantic quality.
Global/project configuration files are not edited or reloaded by this action.
Existing host execution, source-write, passive-provider and approval gates remain
independent. Source bytes are not rewritten.

The UI exposes an ordered list with individual Move/Remove controls. A draft is
pinned to its original project and Session revision; later session changes lock
it until explicit Refresh session. Confirmation retains the original binding.
Display strings use text nodes, and desktop/mobile rendering is separately
identified as synthetic visual acceptance. Broader accessibility, raw logs,
project configuration writes, binding migration, owned shutdown and full
dashboard/STATUS acceptance remain open. Selected global file recovery is
[separately implemented](session-dashboard-documents.md).
