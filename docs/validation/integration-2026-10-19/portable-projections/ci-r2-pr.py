from pathlib import Path
import subprocess,json,hashlib,base64,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');temp=Path.home()/'AppData/Local/Temp';repo='HECer/Yoke-CodeIntelligence';branch='integrate/native-parity-github-app-20261018';path='docs/validation/integration-2026-10-19/portable-projections/index.json'
def sha(b):return hashlib.sha256(b).hexdigest()
def api(route):return json.loads(subprocess.check_output(['gh','api',route],cwd=root))
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root).decode().strip();index=json.loads((root/path).read_bytes());pr=api(f'repos/{repo}/pulls/7');assert pr['state']=='open' and pr['draft'] and pr['head']['sha']==head
old=pr['body'];assert len(old.encode())==23812 and sha(old.encode())=='cc77cf361460f25b11460228f09e4e74e3e93e09f000f6c48491089a95178cc9', 'Review PR body baseline before editing'
marker='<!-- portable-projections-20261019:start -->';assert marker not in old
t=index['finalScopedTotals']
append=f'''

{marker}
Portable Meaning projection checkpoint (2026-10-19)

- `meaning-projection` and its separate Rust API consume native group/join exports as bounded untrusted data, with inspection, node lookup, degree and directed traces. Review/node/policy identities, graph/packet membership, original directed endpoints, evidence references and every ordered hyperedge slot are checked. Parallel observations, selfloops and repeated roles remain distinct.
- Imports do not construct the checked `JoinedGraph` capability or replay captured sources/receipts. Original claims stay nested under `exportMetadata`; top-level `bindingValidation=not_replayed`, `sourceAuthority=false`, `semanticIdentityEstablished=false`, `providerAttestation=unverified`, `modelCalls=0` and `complete=false` remain explicit. Coherent forgeries/omissions cannot be detected from supplied data alone.
- Nine new contracts pass, including actual exports after Store/provider deletion, grouped and two-parent graph qualification, asymmetric traversal, exact independent byte/count/output admission, malformed/duplicate/Unicode/authority/path/deadline cases and CLI no-initialization. The owned local protocol fixtures are not live model or semantic-quality validation.
- Final frozen selected native results: **{t['passed']} passed / {t['failed']} failed / {t['ignored']} existing ignored gates**, across six groups. All-target Clippy, format, 17 Python tests, mandatory typecheck and TypeScript **260 passed / 3 existing skips** pass. **{index['frozenIntegratedInputs']} integrated inputs** remain unchanged; Git/CRLF materialization is recorded explicitly.
- Initial API RED0/1 and intermediate6/1 traversal fixture error are preserved. The latter selected an equal label in another packet; the implementation correctly kept that identity separate. Final tests use the exact relation endpoint and exercise cross-packet/cross-graph distinction.
- One existing archive CLI security fixture remains failed while creating a real Windows symlink (OS1314); it was not weakened or skipped. Final regressions use `--no-fail-fast`. The last change strengthens only the portable tests to close the engine and assert actual removal of the source/Store/provider directory; production/library inputs remain byte-identical to the retained429-test library run, which is not counted twice.
- Evidence: [portable-projections/index.json](https://github.com/{repo}/blob/{head}/{path}) and [contract](https://github.com/{repo}/blob/{head}/native/docs/meaning-projection.md). Counts across attempts are nonadditive. No whole-native rerun, main merge, release or full upstream acceptance is claimed. Semantic alias identity/de-dup, mixed-document/cross-history/workspace contracts, outcome learning and broader R1-R9 work remain open.
<!-- portable-projections-20261019:end -->
'''
body=old+append;bodyfile=temp/'ci-pr-7-body-portable-projections-final.md';bodyfile.write_bytes(body.encode());payload=temp/'ci-r2-pr-payload.json';payload.write_text(json.dumps({'body':body}),encoding='utf-8')
subprocess.run(['gh','api','--method','PATCH',f'repos/{repo}/pulls/7','--input',str(payload)],cwd=root,stdout=subprocess.DEVNULL,check=True)
actual=api(f'repos/{repo}/pulls/7');remote=api(f'repos/{repo}/contents/{path}?ref={head}');raw=base64.b64decode(remote['content']);assert raw==(root/path).read_bytes();assert actual['head']['sha']==head and actual['body']==body and actual['body'].startswith(old)
remotehead=subprocess.check_output(['git','ls-remote','origin','refs/heads/'+branch],cwd=root).decode().split()[0];assert remotehead==head
proof={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'head':head,'prState':actual['state'],'draft':actual['draft'],'bodyBytes':len(body.encode()),'bodySha256':sha(body.encode()),'preservedPrefixBytes':len(old.encode()),'preservedPrefixSha256':sha(old.encode()),'remoteIndexSha256':sha(raw),'remoteBranchHead':remotehead,'checksPolled':False}
(root/'validation/integration-20261018/portable-projections/remote-verification.json').write_text(json.dumps(proof,indent=2)+'\n');print(json.dumps(proof))
