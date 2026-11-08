from pathlib import Path
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=w/'native/src/protocol.rs';s=p.read_text(encoding='utf-8')
old='Steer a persistent host-registered project session, read captured sources and authored notes, or prepare guarded edits. Host authority is configured outside tool arguments.'
new='Steer a persistent host-registered project session, read captured sources and authored notes, or use refactor.prepare for native safeDelete/rename/inline/move plans. Host authority is configured outside tool arguments; application remains separately guarded.'
assert s.count(old)==1;p.write_text(s.replace(old,new),encoding='utf-8')
p=w/'native/docs/agent-sessions.md';s=p.read_text(encoding='utf-8');assert '## Native semantic planning' not in s
s+='''
## Native semantic planning

The native session operation `refactor.prepare` exposes `safeDelete`, `rename`,
`inline` and `move` with their existing native arguments and source-bound plans.
For example, a `session --requests` file can contain:

```json
[
  {
    "operation": "refactor.prepare",
    "args": {
      "operation": "safeDelete",
      "arguments": {
        "relativePath": "module.ts",
        "namePath": "removeMe",
        "propagate": true,
        "propagationPolicy": "captured_es_function_propagation_v2"
      }
    }
  }
]
```

Select provider configuration through the existing host CLI options, for example:

```text
yoke --root C:/project --language-server-config C:/host/servers.json --trust-servers session --id work --requests C:/host/requests.json
```

The same operation is available through `ci_session` in the native session MCP
profile: its `operation` is `refactor.prepare`, and its `args` is the inner object
containing `operation` and `arguments`. Existing native eight-tool and published
facade argument schemas remain unchanged. The session tool description advertises
the operation; no additional tool is required.

The envelope is closed and `arguments` must be an object of at most 64,000
serialized UTF-8 bytes. Only the four planning operations above are supported.
Arguments cannot choose another project, executable, host profile or permission.
Editing mode must be enabled, planning mode disabled, and the active project
writable under its host registration. Query-only projects remain unavailable for
activation. Before starting a provider, the stored session record must still
match the opened session; another client's steering requires resuming the session.
This is an admission check, not a lease over future mode or source changes.

Native validation, original snapshot/provider/source bindings, the caller's
deadline and cancellation remain in force. Preparation does not steer the session
or change source files. It publishes an ordinary guarded plan under the existing
nontransactional publication boundary. Approval, executed verification, workspace
write permission, stale-source checks and conditional recovery remain separate.
Providers still require trusted host registration; requests grant no source
execution or model authority. Unsupported native cases fail explicitly.
'''
p.write_text(s,encoding='utf-8')
p=w/'native/README.md';s=p.read_text(encoding='utf-8');s+='''
The native [session refactoring route](docs/agent-sessions.md#native-semantic-planning)
exposes existing semantic plans through `session --requests` and `ci_session`.
It retains mode, project, current-session, provider and source checks, with
separate approval, verification, workspace writes and recovery.
''';p.write_text(s,encoding='utf-8')
t=Path(__file__).parent;p=t/'ci-session-check.py';s=p.read_text(encoding='utf-8')
assert "'project_session.rs','session_refactor.rs'" in s
s=s.replace("'project_session.rs','session_refactor.rs'","'project_session.rs','protocol.rs','session_refactor.rs'")
p.write_text(s,encoding='utf-8')
print('Session contract, tool description and raw-source capture updated')
