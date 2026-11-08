from pathlib import Path
import subprocess,json,hashlib,io
root=Path.cwd();out=root/'docs/validation/integration-2026-10-19/directory-moves';frozen=json.loads((out/'final/source-before.json').read_bytes());names=sorted(frozen);tracker='docs/parity/native-reconciliation-2026-10-15.json';metadata=json.loads((out/'metadata-after-freeze.json').read_bytes());repair=json.loads((out/'post-freeze-repair-inputs.json').read_bytes());runtime_repairs=repair['normalizedRuntimeDifferences'];repair_hashes=json.loads((out/'development/rename-admission-repair/source-before.json').read_bytes())
raw=subprocess.check_output(['git','cat-file','--batch'],input=(''.join(':'+n+'\n' for n in names)).encode());stream=io.BytesIO(raw);entries=[]
for n in names:
 header=stream.readline().decode().strip().split();assert len(header)==3 and header[1]=='blob',(n,header)
 blob=stream.read(int(header[2]));assert stream.read(1)==b'\n';current=(root/n).read_bytes()
 if n==tracker:
  original=(out/'frozen-tracker.json').read_bytes();assert hashlib.sha256(original).hexdigest()==frozen[n];assert hashlib.sha256(current).hexdigest()==metadata['trackerAfterSha256'];assert current==blob or current.replace(b'\r\n',b'\n')==blob
  baseline=subprocess.check_output(['git','show','eaf9d5995c2370b4873bbd752f1479b0923637a0:'+n]);assert original==baseline or original.replace(b'\r\n',b'\n')==baseline;old=json.loads(original);new=json.loads(current);assert all(old[k]==new[k] for k in ['inventoryLedger','historicalInventory','ledgerSummary']);materialization='post_freeze_thematic_documentation_only'
 elif n in runtime_repairs:
  original=(out/'development/pre-repair-full-source'/n).read_bytes();assert hashlib.sha256(original).hexdigest()==frozen[n]
  repaired=(out/'development/rename-admission-repair/source'/n).read_bytes();assert hashlib.sha256(repaired).hexdigest()==repair_hashes[n];assert current.replace(b'\r\n',b'\n')==repaired.replace(b'\r\n',b'\n');assert current==blob or current.replace(b'\r\n',b'\n')==blob;materialization='scoped_post_freeze_runtime_repair'
 else:
  assert hashlib.sha256(current).hexdigest()==frozen[n],n;materialization='raw_equal' if current==blob else 'crlf_only_checkout_materialization';assert current==blob or current.replace(b'\r\n',b'\n')==blob,n
 entries.append({'path':n,'frozenSha256':frozen[n],'gitBlobSha256':hashlib.sha256(blob).hexdigest(),'gitObject':header[0],'materialization':materialization})
assert not stream.read();(out/'source-materialization.json').write_bytes((json.dumps({'allCurrentInputsBoundToRecordedRuns':True,'count':len(entries),'runtimeRepairs':runtime_repairs,'metadataExceptions':[tracker],'entries':entries},indent=2)+'\n').encode());p=out/'index.json';index=json.loads(p.read_bytes());index['sourceMaterialization']='source-materialization.json';index['metadataAfterFreeze']='metadata-after-freeze.json';index['artifacts']=[]
for artifact in sorted(out.rglob('*')):
 if artifact.is_file() and artifact!=p:
  b=artifact.read_bytes();index['artifacts'].append({'path':artifact.relative_to(out).as_posix(),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
p.write_bytes((json.dumps(index,indent=2,ensure_ascii=False)+'\n').encode());print(json.dumps({'frozenInputs':len(entries),'rawEqual':sum(e['materialization']=='raw_equal' for e in entries),'crlfOnly':sum(e['materialization']=='crlf_only_checkout_materialization' for e in entries),'documentedRuntimeRepairs':len(runtime_repairs),'documentedMetadataExceptions':1,'artifacts':len(index['artifacts'])}))
