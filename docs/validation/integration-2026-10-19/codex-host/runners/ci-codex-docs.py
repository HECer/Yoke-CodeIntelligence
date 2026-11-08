from pathlib import Path
import json,hashlib,shutil
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/codex-host';source=json.loads((base/'source-integration.json').read_bytes());sha=lambda b:hashlib.sha256(b).hexdigest();tracker=root/'docs/parity/native-reconciliation-2026-10-15.json';old_raw=tracker.read_bytes();(base/'frozen-tracker.json').write_bytes(old_raw);old=json.loads(old_raw);value=json.loads(old_raw);r6=next(s for s in value['prioritizedRemainingScopes'] if s['id']=='R6');paths=[*source['paths'],'docs/native-codex-host.md','docs/validation/integration-2026-10-19/codex-host/index.json'];totals=source['nativeTotals']
for path in paths:
 if path not in r6['evidence']:r6['evidence'].append(path)
r6['missingBehavior']='Bounded generic-agents-v1 installation and v2 source-security/output-ownership recovery retain their accepted matrix and legacy identities. A separate explicit Windows codex-mcp-v1 profile now manages one receipt-owned config.toml stdio MCP table, with closed host selection, physical source/launch bindings, exact human TOML preservation, independent host-write authority and profile-specific canonical initialization/v1/v2 recovery. Actual CodexCLI0.163.0 parses the installed entry, starts the native server and dispatches an exact source read through its MCP client without a model turn; uninstall and actual process interruption/recovery have owned-fixture evidence. Other branded clients, hooks, interactive IDE/model tool selection, other client releases and non-Windows runtime certification remain open. Hard cancellation/power-loss/universal recovery/performance/full upstream certification remain unestablished; historical unknown-output refusal and terminal rolled_back security boundaries remain.'
r6['suggestedScope']='Continue remaining branded schema and hook adapters plus non-Windows qualification. Preserve generic legacy serialized identities, the distinct Codex profile and physical source/launch bindings, exact byte ownership/security, canonical recovery and independent host-write gates. No default model or analyzed project-code execution follows from a client config or imported plan.'
r6['codexMcpCheckpoint']={'index':paths[-1],'newContractCases':{'adapterOwnershipAndInitialization':6,'actualProcessRecovery':3,'hostCli':3},'nativeGroups':13,'finalSelectedTotals':totals,'actualClient':'CodexCLI0.163.0','actualPlatform':'Windows','actualClientMcpStartupAndToolDispatch':True,'actualModelTurnTested':False,'unicodeCrLfSourceExact':True,'legacyGenericIdentitiesPreserved':True,'guardedInstallationPlatforms':['windows'],'fullUpstreamContractCertified':False,'wholeNativeRerun':False}
assert not any(c['id']=='native_codex_managed_mcp_install' for c in value['reconciledCapabilities'])
value['reconciledCapabilities'].append({'id':'native_codex_managed_mcp_install','behavior':'Explicit Windows codex-mcp-v1 host-install adapter over the existing lossless TOML planner and guarded installation/security/v2 journal. Closed host-selected config/source/state roots and launch paths bind physical objects; legacy generic None bindings/serialized identities stay unchanged. A receipt-owned mcp_servers.yoke_<label> entry uses fixed native argv, explicit source cwd and six discovery names without granting native host policy. Planning/status never index or create source state. Exact human tables, original empty-file semantics, stale source roots, cross-profile authority, lost stdout and canonical initialization/v1/v2 recovery are checked. Actual CodexCLI0.163.0 parses/starts the installed MCP server and dispatches an exact UnicodeCRLF read with no model turn; uninstall preserves unrelated tables.','evidence':paths,'remainingReview':'Managed installation/recovery remain Windows-only. This finite profile selects separate nonnested source/config/state roots and does not manage project-local .codex beneath its source. Client tool discovery settings are not native execution/write/source authority. Only CodexCLI0.163.0 and its owned MCP-client API path are live qualified; interactive IDE/model tool selection, other releases/clients/hooks and non-Windows are untested. Multi-file atomic visibility, hard cancellation/power-loss durability, universal recovery/performance, full upstream/main/release remain open. Earlier host-platform prerequisite failures remain independently recorded.','fullContractCertified':False})
for key in ['inventoryLedger','historicalInventory','ledgerSummary']:assert value[key]==old[key]
tracker.write_bytes((json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode());shutil.copyfile(Path(__file__).parent/'ci-codex-host-guide.md',root/'docs/native-codex-host.md')
status=root/'docs/STATUS.md';text=status.read_text(encoding='utf-8');marker='Windows ZIP and VSIX development packages';assert text.count(marker)==1
paragraph=f'''[Managed native Codex MCP installation](validation/integration-2026-10-19/codex-host/index.json)
adds a separate explicit Windows `codex-mcp-v1` profile. Closed host selections
bind source/configuration/state directories and launch objects without home
discovery. The existing lossless TOML planner and guarded installation journals
preserve human bytes, stale conditions and ownership. Generic v1 serialized
identities stay unchanged. Initialization and both recovery schemas replay the
selected profile's canonical edit. See the [workflow](native-codex-host.md).

Actual **Codex CLI0.163.0** parses the installed command/argv/cwd/tool list,
starts its configured native server and dispatches an exact Unicode/CRLF source
read through the real MCP client. No model turn is started and the owned
loopback model endpoint receives no request. All owned processes exit with an
empty checked job after the bounded cleanup grace. Uninstall preserves human
tables; process-exit/resume, kill/rollback and stale-source recovery cases pass.

There are **12 new cases**. The final **13 selected native groups** record
**{totals['passed']} passed / {totals['failed']} failed / {totals['ignored']} existing ignored gates**.
All-target Clippy with warnings denied, build and format pass. Native/live
input proofs cover **727 files** and the exact final binary. Mandatory
integrated checks are retained in the indexed evidence. Earlier test compiler,
canonical-recovery and public-status assertion failures remain visible and
incremental runs are nonadditive. The earlier media OS1314 prerequisite failure
is outside this selection; no whole-native or platform-wide pass follows.

Other branded clients/hooks, other Codex versions, interactive IDE/model tool
selection, project-local nested config roots, non-Windows and full upstream
qualification remain open. Multi-file visibility/power-loss guarantees are not
established. WiderR1-R9, unattended production, main and release remain open.

'''
status.write_text(text.replace(marker,paragraph+marker),encoding='utf-8',newline='\r\n')
architecture=root/'docs/ARCHITECTURE.md';text=architecture.read_text(encoding='utf-8');text+='\nThe separate managed Codex MCP profile extends `host_install` through an optional\nclosed adapter binding; absent bindings retain released generic identities.\nPhysical source/config/state objects and ordinary launch paths remain host\nselections. The existing lossless TOML planner supplies the exact owned table,\nand planning, initialization, both recovery schemas and receipts use the same\nprofile-specific canonical patch. No imported plan or client discovery list\ncreates host/source/execution authority. See the\n[Codex workflow](native-codex-host.md) and\n[indexed validation](validation/integration-2026-10-19/codex-host/index.json).\n';architecture.write_text(text,encoding='utf-8',newline='\r\n')
readme=root/'README.md';text=readme.read_text(encoding='utf-8');needle='## TypeScript advanced runtime';assert text.count(needle)==1;text=text.replace(needle,'An explicit [managed Codex MCP profile](docs/native-codex-host.md) installs one\nreviewed, receipt-owned TOML entry with physical source/launch bindings and\nguarded recovery. The original generic AGENTS profile keeps its identities.\nActual Codex0.163 MCP-client reads have owned Windows evidence.\n\n'+needle);readme.write_text(text,encoding='utf-8',newline='\r\n')
(base/'documentation-proof.json').write_text(json.dumps({'historicalInventoryLedgerAndSummaryUnchanged':True,'oldTrackerSha256':sha(old_raw),'currentTrackerSha256':sha(tracker.read_bytes()),'mainOrReleaseVerified':False},indent=2)+'\n',encoding='utf-8');print(json.dumps({'historicalLedgerUnchanged':True,'newCases':12,'nativeTotals':totals}),flush=True)
