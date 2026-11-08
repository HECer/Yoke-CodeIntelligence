from pathlib import Path
import subprocess,json,hashlib,datetime
root=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CodeIntelligence');base=root/'validation/integration-20261018/mixed-meaning-claims'
sha=lambda b:hashlib.sha256(b).hexdigest()
def api():return json.loads(subprocess.check_output(['gh','api','repos/HECer/Yoke-CodeIntelligence/pulls/7'],cwd=root))
before=api();raw=before['body'].encode()
assert len(raw)==6420 and sha(raw)=='c3e5ae3573094073ee6791153608fa92fe13d6988c94bd602c05d92c0d618432'
assert before['head']['sha']=='da45d2591cfb9f208e87d742eadc17a65fc8f50a' and before['draft']
text=raw.decode();old='|Integrated source|744 inputs unchanged; all 738 native inputs match exactly or only by CRLF checkout materialization|'
assert old in text;text=text.replace(old,'|Integrated source|745 inputs unchanged; all 739 native inputs match exactly or only by CRLF checkout materialization|')
old='The actual CLI executable is hashed before/after use and matches the final build.'
assert old in text;text=text.replace(old,'Earlier TS/LSP CLI checkpoints hash their executable before/after use against\ntheir own final build. The latest default PDF-worker case is covered by its\nunchanged source freeze; no separate before/after call-hash attestation is claimed.')
body=text.encode();file=Path(__file__).parent/'ci-pr-7-body-mixed-meaning-inputs-corrected.md';file.write_bytes(body)
(base/'pr-description-correction-before.md').write_bytes(raw)
(base/'pr-description-correction-after.md').write_bytes(body)
subprocess.run(['gh','pr','edit','7','--repo','HECer/Yoke-CodeIntelligence','--body-file',str(file)],cwd=root,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
after=api();assert after['body'].encode()==body and after['head']['sha']==before['head']['sha']
result={'verifiedAt':datetime.datetime.now(datetime.timezone.utc).isoformat(),'head':after['head']['sha'],
    'beforeBytes':len(raw),'beforeSha256':sha(raw),'afterBytes':len(body),'afterSha256':sha(body),
    'changes':['Correct latest integrated/native inventory row744/738 to745/739','Scope before/after CLI executable hashing to earlier TS/LSP checkpoint; latest PDF case has no separate call-hash attestation']}
(base/'pr-description-correction.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
