from pathlib import Path
import subprocess,json,hashlib,base64,time,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/codex-host';archive=root/'docs/validation/integration-2026-10-19/codex-host';temp=Path(__file__).parent;repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';sha=lambda b:hashlib.sha256(b).hexdigest()
def run(args):return subprocess.check_output(args,cwd=root)
def api(path):return json.loads(run(['gh','api',path]))
assert not run(['git','status','--porcelain']);head=run(['git','rev-parse','HEAD']).decode().strip();source=json.loads((base/'source-integration.json').read_bytes());index_raw=(archive/'index.json').read_bytes();old=(temp/'ci-pr-7-body-falkor-final.md').read_bytes();assert len(old)==44552 and sha(old)=='9054f0eaddc12ba4b3c03a6c602005f77d102765392c2ce42c1ac4bc9ff602ce'
before=api('repos/'+repo+'/pulls/7');assert before['head']['sha']=='2e943bff9c64fea2384251bff3ad0cb74c8565cd' and before['body'].encode()==old;assert before['state']=='open' and before['draft'] and before['base']['ref']=='main';marker='<!-- codex-host-20261020:start -->';assert marker.encode() not in old
addition=f'''\n\n{marker}
### Managed Codex MCP installation — Code Intelligence native0.5.0 development

Adds a separate Windows codex-mcp-v1 profile over the guarded host installer and
lossless TOML planner. Explicit source/config/state directories and launch paths
bind physical objects; absent adapter bindings keep generic v1 serialized IDs.
One receipt-owned MCP table preserves human bytes and separate appended tables.
Independent host-write grants, stale/source/security checks and exact canonical
initialization/v1/v2 recovery remain mandatory; imported plans confer no grants.

Actual CodexCLI0.163.0 parses the installed command/argv/cwd/six discovery names,
starts the native server and dispatches an exact Unicode/CRLF source read via
its real MCP client. Uninstall preserves human servers. No model turn starts;
the owned loopback model endpoint receives zero requests. The final owned
process job empties naturally within the bounded grace after root exit.

- Source `{source['sourceCommit']}`; runtime merge `{source['runtimeCommit']}`.
- **12 new cases** (6adapter/initialization,3actualprocessrecovery,3CLI).
- Final **13 selected native groups: 558 passed / 0 failed / 8 ignored**.
  This selection excludes the independently recorded mediaOS1314 prerequisite;
  no whole-native/whole-platform pass follows.
- All-target Clippy with warnings denied, build/format, mandatory typecheck,
  **260 TypeScript tests / 3 skips** and **17 Python tests** pass.
  **733 integrated / 727 native inputs** and final live binary are frozen.
- [Workflow](https://github.com/{repo}/blob/{head}/docs/native-codex-host.md)
  and [indexed evidence](https://github.com/{repo}/blob/{head}/docs/validation/integration-2026-10-19/codex-host/index.json).
  Index SHA-256 `{sha(index_raw)}`.

Raw compiler/test assumptions and the actual canonical-recovery regression are
retained with their corrections. The profile selects separate nonnested source/
config/state roots; project-local nested configs, other clients/hooks/releases,
interactive IDE/model tool selection, non-Windows and full upstream qualification
remain open. Multi-file atomic visibility/power-loss guarantees are unestablished.
BroaderR1-R9/main/release remain open and work continues. All previous44552body
bytes and historical inventory/ledger remain preserved.
<!-- codex-host-20261020:end -->
'''.encode();body=old+addition;assert len(body)<65536;body_file=temp/'ci-pr-7-body-codex-final.md';body_file.write_bytes(body);subprocess.run(['git','push','origin',f'HEAD:refs/heads/{branch}'],cwd=root,check=True);subprocess.run(['gh','pr','edit','7','--repo',repo,'--body-file',str(body_file)],cwd=root,check=True);time.sleep(2)
after=api('repos/'+repo+'/pulls/7');actual_body=after['body'].encode();assert after['head']['sha']==head and actual_body==body and actual_body.startswith(old);assert after['state']=='open' and after['draft'] and after['base']['ref']=='main';assert run(['git','ls-remote','--heads','origin',f'refs/heads/{branch}']).decode().split()[0]==head;remote=api('repos/'+repo+'/contents/docs/validation/integration-2026-10-19/codex-host/index.json?ref='+branch);assert base64.b64decode(remote['content'])==index_raw
receipt={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'url':after['html_url'],'head':head,'branch':branch,'state':'open','draft':True,'base':'main','bodyBytes':len(actual_body),'bodySha256':sha(actual_body),'priorBodyBytes':len(old),'priorBodySha256':sha(old),'priorBodyPreserved':True,'indexSha256':sha(index_raw),'remoteIndexRawEqual':True,'sourceCommit':source['sourceCommit'],'runtimeCommit':source['runtimeCommit'],'mainOrReleaseVerified':False};(base/'remote-verification.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
