from pathlib import Path
import json,hashlib,shutil,subprocess
w=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');raw=w/'validation/integration-20261018/r6-json-mcp';out=w/'docs/validation/integration-2026-10-21/json-mcp-hosts'
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'));sha=lambda b:hashlib.sha256(b).hexdigest()
r=read(raw/'guard-r3/result.json');assert r['allChecksPassed'] and r['sourceUnchanged']
checks=r['checks'];passed=sum(int(t[0]) for c in checks for t in c['totals']);ignored=sum(int(t[2]) for c in checks for t in c['totals']);assert passed==175 and ignored==1
live=read(raw/'client-file-live-r2/result.json');assert live['completed'] and live['clientHealthConnected'] and live['originalConfigBytesRestored'] and live['sourceBytesUnchanged']
assert live['cliSha256']==r['cliSha256'] and sha((w/'target/debug/yoke.exe').read_bytes())==r['cliSha256']
before=read(raw/'guard-r3/source-after.json');assert all(sha((w/n).read_bytes())==h for n,h in before.items())
prev=read(raw/'guard-r2/source-after.json');changed=[n for n in before if before[n]!=prev[n]];assert changed==['native/src/host_output_recovery_tests.rs'] and before.keys()==prev.keys()
log=(raw/'guard-r3/retained.log').read_text(encoding='utf-8');assert 'pass 260' in log and 'fail 0' in log and 'skipped 3' in log
out.mkdir(parents=True,exist_ok=False)
def copy(src,rel):
    dest=out/rel;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(src,dest);assert src.read_bytes()==dest.read_bytes()
for phase in sorted(raw.iterdir()):
    if phase.is_dir():
        for f in sorted(phase.iterdir()):
            assert f.is_file();copy(f,Path(phase.name)/f.name)
for name in ['ci-r6-host-guard.py','ci-r6-json-patch-check.py','ci-r6-claude-client-prep.py','ci-r6-claude-help.py','ci-r6-claude-live.py','ci-r6-claude-file-live.py']:
    copy(Path.home()/'AppData/Local/Temp'/name,Path('controllers')/name)
copy(Path(__file__),Path('archive-and-docs.py'))
summary={'baseCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=w).decode().strip(),'nativeScopedTotals':{'passed':passed,'failed':0,'ignored':ignored},'selectedChecks':checks,'nativeImplementationUnchangedDuringAndAfterChecks':True,'r2ToR3ChangedOnly':changed,'cliSha256':r['cliSha256'],'actualClaudeManagedFileHealth':live,'retainedPassed':260,'retainedSkipped':3,'fullStatusCertified':False,'remaining':['Explicit --mcp-config interactive session','Claude tools dispatch/model selection','Cursor/Desktop runtime qualification','Hooks/statuslines','Non-Windows host installation','Broader R1-R9 and platform/upstream acceptance']}
(out/'checkpoint.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf-8',newline='\n')
(out/'README.md').write_text('''# Guarded JSON MCP host profiles

The [checkpoint](checkpoint.json) records 175 scoped Rust tests passed and one
explicit child worker ignored, plus format, all-target Clippy with warnings
denied, typecheck and TypeScript 260 passed / 3 existing skips. The final guard
uses frozen inputs. Only recovery test code differs from the preceding green
run. These overlapping runs are not added together. Documentation follows checks.

Actual Claude Code 2.1.306 reads the managed `.mcp.json` from an owned temporary
working directory, with only this server approved in isolated client settings.
Both `mcp get` and `mcp list` report Connected. Exact generated entry import into
private user scope also connects. Native install/uninstall restores original
configuration and source bytes. All processes start suspended, enter their own
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
''',encoding='utf-8',newline='\n')
index=[{'path':str(f.relative_to(out)).replace('\\','/'),'bytes':f.stat().st_size,'sha256':sha(f.read_bytes())} for f in sorted(out.rglob('*')) if f.is_file()]
(out/'index.json').write_text(json.dumps({'files':index,'copiedRawFilesByteExact':True},indent=2)+'\n',encoding='utf-8',newline='\n')
def edit(name,old,new):
    p=w/name;s=p.read_text(encoding='utf-8-sig');assert s.count(old)==1,(name,old[:70]);p.write_text(s.replace(old,new),encoding='utf-8',newline='\n')
p=w/'docs/native-json-hosts.md';assert not p.exists();p.write_text((Path.home()/'AppData/Local/Temp/ci-r6-json-host-doc.md').read_text(encoding='utf-8-sig'),encoding='utf-8',newline='\n')
edit('docs/STATUS.md','## Current integration work\n','''## Current integration work

Explicit [JSON MCP host profiles](native-json-hosts.md) now manage receipt-owned
Cursor `mcp.json`, Claude Code external `.mcp.json` and Claude Desktop config
entries through the guarded Windows installer. Strict unique-key JSON patches
create missing parents/files and preserve foreign values, Unicode/BOM/CRLF and
human formatting. Exact profile/source/launch bindings, independent write gates,
no-adoption rules and v1/v2 recovery remain mandatory. Client interpolation paths
are refused; generic/Codex serialized identities stay unchanged.

The final scoped guard passes 175 Rust tests with one explicit child worker
ignored, format, all-target Clippy, typecheck and TypeScript 260/3 existing skips.
Three new process-interruption functions exercise each JSON profile: partial
exit/resume/uninstall, prepared forced kill/rollback of the original file object,
and replaced-source refusals without state changes. Actual Claude Code 2.1.306
loads the managed file in an isolated test directory and reports Connected;
private import of the exact generated entry also connects. No login or model turn
is requested. All validation process jobs are verified empty; configuration and
source bytes roundtrip unchanged. Failed development phases remain archived.

Claude status subcommands ignore global `--mcp-config`; the passing file gate
loads `.mcp.json` from the owned client working directory with only its test
server approved in private settings. Interactive explicit-file loading, model
selection/tool dispatch, Cursor/Desktop runtimes, hooks/statuslines and broader
R1-R9/Windows/current-head hosted/upstream qualification remain open. See the
[exact evidence](validation/integration-2026-10-21/json-mcp-hosts/README.md).
''')
edit('docs/REMAINING_WORK.md','Generic managed installation, guarded v1/v2 recovery, the separate Codex MCP profile and native session semantic planning. | Other branded client schemas and hook/statusline adapters, their actual client gates and remaining platform qualification.', 'Generic managed installation, guarded v1/v2 recovery, separate Codex and strict JSON Cursor/Claude Code/Claude Desktop MCP profiles, actual isolated Claude2.1.306 managed-file health and native session semantic planning. | Claude explicit-file session/tool dispatch and Cursor/Desktop runtime qualification; further branded schemas, hook/statusline adapters and remaining platform qualification.')
edit('docs/ARCHITECTURE.md','## Native Rust ownership\n','''## Native Rust ownership

Explicit JSON MCP clients share guarded host installation through closed typed
adapter bindings and distinct profile identities. Scoped strict-JSON ownership
tracks exact server member bytes and created shells; configuration remains
separate from source and private journal roots. Client launches are literal
host-selected argv, with no approval/model defaults. The same canonical edits
and physical bindings govern ordinary apply and interrupted recovery. See the
[JSON client contract](native-json-hosts.md).
''')
edit('README.md','An explicit [managed Codex MCP profile]', 'Explicit [managed JSON MCP profiles](docs/native-json-hosts.md) select Cursor or\nClaude configuration, with exact ownership and guarded Windows recovery.\n\nAn explicit [managed Codex MCP profile]')
edit('native/docs/host-install.md','Other branded clients, hooks and non-Windows certification remain\nopen; the generic profile alone makes no branded-client claim.', 'Separate [strict JSON MCP profiles](../../docs/native-json-hosts.md) now manage\nCursor, Claude Code external configuration and Claude Desktop schemas using the\nsame guarded machinery. Actual isolated Claude2.1.306 file health is finite\nclient evidence; other runtime/hook/non-Windows qualification remains open. The\ngeneric profile alone makes no branded-client claim.')
p=w/'native/docs/host-config.md';s=p.read_text(encoding='utf-8');p.write_text(s+'''
## Strict JSON MCP entries

`json_mcp_patch` is a separate versioned format for a named `mcpServers` member.
It admits strict unique-key JSON with an optional BOM, creates absent containers
or documents, and records exact owned entry and created-shell boundaries.
Ownership is scoped to the decoded member path, so identical bytes in an unrelated
nested value do not conflict. Removal preserves foreign members and whitespace,
removing only its own entry and a required comma. Created shells disappear only
while their exact owned wrapping bytes remain unchanged. Modified entries and
unowned existing names refuse. Serialized ownership/patches never grant writes.
See the [JSON host contract](../../docs/native-json-hosts.md) for bounds, physical
bindings, Windows recovery, client evidence and remaining runtime qualifications.
''',encoding='utf-8',newline='\n')
for n,h in before.items():
    if sha((w/n).read_bytes())!=h:assert n in ['native/docs/host-config.md','native/docs/host-install.md'],n
fixture=w/'native/tests/fixtures/postgres-catalog-response.jsonl';assert fixture.stat().st_size==2779 and sha(fixture.read_bytes())=='f67a05349a6cedf55222afd3bb3231c169d382dd4b078d76544daac517530b47'
print(json.dumps({'archivedFiles':len(index)+1,'archivedBytes':sum(x['bytes'] for x in index),'nativePassed':passed,'nativeIgnored':ignored,'runtimeSourceUnchanged':True,'documentationUpdated':True}))
