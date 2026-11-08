from pathlib import Path
import os,tempfile,subprocess,json,hashlib,time
root=Path.cwd();base=Path(os.environ['LOCALAPPDATA'])/'YokeValidation';owned=Path(tempfile.mkdtemp(prefix='discussions-live-',dir=base));out=root/'validation/integration-20261018/discussions-development/live-final';out.mkdir(exist_ok=False)
receipts=[]
for label,repo,repo_id,pulls in [('own-empty','hecer/yoke-codeintelligence',1417787673,[]),('upstream-closed','trailhq/trail-code',1288325627,[623]),('actual-reviews','rust-lang/rust',724712,[163746])]:
 profile={'read':{'allowNetwork':True,'allowedRepos':[repo],'maxPages':5,'maxItems':1000,'maxRequests':40,'maxResponseBytes':8*1024*1024,'aggregateResponseBytes':32*1024*1024},'allowDiscussionText':True}
 request={'schemaVersion':1,'repository':repo,'repositoryId':repo_id,'pullRequests':pulls}
 profile_path=owned/(label+'-profile.json');request_path=owned/(label+'-request.json');capture_path=owned/(label+'-capture.json')
 profile_path.write_text(json.dumps(profile),encoding='utf-8');request_path.write_text(json.dumps(request),encoding='utf-8')
 cmd=[str(root/'target/debug/yoke.exe'),'--timeout','60000','--max-bytes','8000000','github-discussions','capture','--profile',str(profile_path),'--request',str(request_path),'--allow-network','--allow-discussion-text']
 start=time.monotonic()
 with capture_path.open('wb') as stdout,(out/(label+'.stderr.log')).open('wb') as stderr:r=subprocess.run(cmd,cwd=owned,stdout=stdout,stderr=stderr,timeout=90,creationflags=subprocess.CREATE_NO_WINDOW)
 raw=capture_path.read_bytes();receipt={'label':label,'command':cmd,'exitCode':r.returncode,'elapsedSeconds':round(time.monotonic()-start,3),'outputBytes':len(raw),'outputHash':hashlib.sha256(raw).hexdigest(),'sourceStoreCreated':(owned/'.yoke').exists(),'rawPayloadPublished':False}
 if r.returncode==0:
  c=json.loads(raw);p=c['payload'];counts={}
  for pull in p['pulls']:
   for observation in pull['observations']:counts[observation['kind']]=counts.get(observation['kind'],0)+1
  receipt.update({'captureId':c['id'],'repository':p['repository'],'repositoryId':p['repositoryId'],'pullNumbers':[x['number'] for x in p['pulls']],'unacquiredPulls':p['unacquiredPulls'],'observations':p['observations'],'observationsByKind':counts,'requests':p['requests'],'responseBytes':p['responseBytes'],'coverageComplete':p['coverageComplete'],'transport':p['transport'],'atomic':p['atomic']})
 receipts.append(receipt);(out/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n',encoding='utf-8');print(json.dumps(receipt),flush=True)
(out/'scope.json').write_text(json.dumps({'ownedDirectory':str(owned),'scope':'Anonymous native HTTPS GET only. Own repository has no closed PRs at selection time; actual discussion/review acquisition uses canonical public Trail-Code PR623 and rust-lang/rust PR163746. Raw texts stay in owned external validation directory; no source Store, comment, review, deployment or service send.'},indent=2)+'\n',encoding='utf-8')
raise SystemExit(any(x['exitCode']!=0 or x['sourceStoreCreated'] for x in receipts))
