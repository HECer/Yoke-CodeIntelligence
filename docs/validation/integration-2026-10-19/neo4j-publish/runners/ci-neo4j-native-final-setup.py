from pathlib import Path
temp=Path.home()/'AppData/Local/Temp'
source=(temp/'ci-neo4j-native-tls-r2.py').read_text().replace('neo4j-publish/native-tls-r2','neo4j-publish/native-final-r1')
init="""work=Path('C:/Users/Lomi/Documents/NN-Developed/Yoke-CI-r1-attach')
def inventory():
 names=sorted(set(subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','-z','native','Cargo.toml','Cargo.lock','.gitattributes'],cwd=work).decode().split('\\0')))
 return {n:hashlib.sha256((work/n).read_bytes()).hexdigest() for n in names if n and (work/n).is_file()}
source_before=inventory();(out/'source-before.json').write_text(json.dumps(source_before,indent=2)+'\\n',encoding='utf-8');binary_before=hashlib.sha256((primary/'target/debug/yoke.exe').read_bytes()).hexdigest()
"""
source=source.replace('def run(args,timeout=40):',init+'def run(args,timeout=40):')
source=source.replace("   commands=[];created_credentials=[];proxy=None;proxy_thread=None;proxy_events=[];drop_next={'armed':False}","   (out/'owned-fixture.rs').write_bytes((owned_source/'owned.rs').read_bytes())\n   commands=[];created_credentials=[];proxy=None;proxy_thread=None;proxy_events=[];drop_next={'armed':False};race={'armed':False};race_barrier=threading.Barrier(2)")
source=source.replace("     forwarded=urllib.request.Request", "     if race['armed'] and statement.startswith('CYPHER 5 MATCH (scope:YokeNativeScope'):race_barrier.wait(timeout=20)\n     forwarded=urllib.request.Request")
source=source.replace("    assert (code==0)==success,(name,code,'see bounded local stderr')", "    if success is not None:assert (code==0)==success,(name,code,'see bounded local stderr')")
source=source.replace("     count=len(proxy_events);cli(prefix+'-recover'", "     historical=folder/'historical-pending';historical.mkdir();shutil.copyfile(pj,historical/pj.name)\n     count=len(proxy_events);cli(prefix+'-recover'")
insert="""    # Compare original saved graph/evidence/observations against actual rows.
    for prefix,receipt in [('direct-native',direct),('tls-native',encrypted),('response-loss-native',recovered)]:
     plan=json.loads((out/prefix/'publication.json').read_bytes());payload=plan['payload'];scope=plan['scopeKey'];revision=plan['revisionId']
     actual=query(prefix+'-all-original-elements','MATCH (n:YokeNativeElement {scopeKey:$scope,revisionId:$revision}) RETURN n.category,n.rowJson',{'scope':scope,'revision':revision})['data']['values']
     for category,field in [('entity','entities'),('evidence','evidence')]:
      assert {json.loads(r[1])['id']:json.loads(r[1]) for r in actual if r[0]==category}=={r['id']:r for r in payload[field]}
     header=next(json.loads(r[1]) for r in actual if r[0]=='revision')
     for field in ['sourceFiles','coverage','diagnostics','graphHash','payloadHash']:assert header[field]==payload[field]
     observations=query(prefix+'-all-original-observations','MATCH (a:YokeNativeElement)-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$scope,revisionId:$revision}]->(b:YokeNativeElement) RETURN a.sourceId,b.sourceId,b.sourceTargetWasNull,r.rowJson',{'scope':scope,'revision':revision})['data']['values']
     assert {json.loads(r[3])['id']:json.loads(r[3]) for r in observations}=={r['id']:r for r in payload['observations']}
     for r in observations:
      raw=json.loads(r[3]);assert r[0]==raw['source'];assert (r[1]==raw['target'] and r[2] is None) if raw['target'] is not None else (r[1] is None and r[2] is True)
    # Both legitimate native updates reach send with the same old revision.
    config=out/'response-loss-native/target.json';race_plans=[];connect=['--allow-network','--allow-credential-lookup','--allow-loopback-http']
    for label in ['A','B']:
     code=('fn candidate_'+label+'(){}\\r\\n').encode();(owned_source/('race-'+label+'.rs')).write_bytes(code);(out/('owned-race-'+label+'.rs')).write_bytes(code)
     snapshot=cli('race-'+label+'-index',['--root',owned_source,'index'])['snapshotId'];folder=out/('race-'+label);folder.mkdir();path=folder/'publication.json'
     cli('race-'+label+'-prepare',['neo4j-publish','prepare','--config',config,'--source-root',owned_source,'--snapshot-id',snapshot,'--expected-revision',recovered['revisionId'],'--output',path,'--allow-output','--allow-source-coordination']);plan=json.loads(path.read_bytes());journal=folder/('neo4j-'+plan['planId']+'.jsonl')
     cli('race-'+label+'-approve',['neo4j-publish','approve','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-approval','--allow-journal']);race_plans.append((label,path,plan,journal))
    race_start=len(proxy_events);race['armed']=True
    def contender(entry):
     label,path,plan,journal=entry
     return cli('race-'+label+'-apply',['neo4j-publish','apply','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-write','--allow-journal']+connect,success=None)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:outcomes=list(pool.map(contender,race_plans))
    race['armed']=False;assert sum(isinstance(r,dict) for r in outcomes)==1;assert sum(e['publication'] for e in proxy_events[race_start:])==2
    winner=race_plans[next(i for i,r in enumerate(outcomes) if isinstance(r,dict))];loser=race_plans[next(i for i,r in enumerate(outcomes) if not isinstance(r,dict))]
    state=query('concurrent-final-state','MATCH (s:YokeNativeScope {key:$scope}) RETURN s.activeRevision,s.lockCounter,COUNT { MATCH (n:YokeNativeElement {scopeKey:$scope,revisionId:$loser}) },COUNT { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$scope,revisionId:$loser}]->() }',{'scope':recovered['scopeKey'],'loser':loser[2]['revisionId']})['data']['values'];assert state==[[winner[2]['revisionId'],2,0,0]]
    before=len(proxy_events);denied=cli('race-loser-recovery-refused',['neo4j-publish','recover','--plan',loser[1],'--expected-plan-id',loser[2]['planId'],'--journal',loser[3],'--allow-recovery','--allow-journal']+connect,success=False);assert 'result_still_unknown_no_write_replayed' in denied and not any(e['publication'] for e in proxy_events[before:])
    original=out/'response-loss-native/publication.json';historical=out/'response-loss-native/historical-pending'/('neo4j-'+recovered['planId']+'.jsonl')
    cli('historical-recovery-after-newer-publication',['neo4j-publish','recover','--plan',original,'--expected-plan-id',recovered['planId'],'--journal',historical,'--allow-recovery','--allow-journal']+connect)
    # A superficially matching header cannot hide extra scoped rows.
    extra=hashlib.sha256(secrets.token_bytes(32)).hexdigest();query('insert-owned-count-conflict','CREATE (n:YokeNativeElement {key:$key,scopeKey:$scope,revisionId:$revision})',{'key':extra,'scope':recovered['scopeKey'],'revision':winner[2]['revisionId']})
    label,path,plan,_=winner;folder=out/'count-conflict';folder.mkdir();journal=folder/('neo4j-'+plan['planId']+'.jsonl');cli('count-conflict-approve',['neo4j-publish','approve','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-approval','--allow-journal'])
    denied=cli('count-conflict-apply-refused',['neo4j-publish','apply','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-write','--allow-journal']+connect,success=False);assert 'query_refused' in denied
    unchanged=query('count-conflict-counter-unchanged','MATCH (s:YokeNativeScope {key:$scope}) RETURN s.lockCounter',{'scope':recovered['scopeKey']})['data']['values'];assert unchanged==[[2]]
    denied=cli('count-conflict-recovery-refused',['neo4j-publish','recover','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-recovery','--allow-journal']+connect,success=False);assert 'result_still_unknown_no_write_replayed' in denied
    query('remove-only-owned-extra-node','MATCH (n:YokeNativeElement {key:$key}) DELETE n',{'key':extra})
    cli('count-conflict-postcondition-restored-read-only',['neo4j-publish','recover','--plan',path,'--expected-plan-id',plan['planId'],'--journal',journal,'--allow-recovery','--allow-journal']+connect)
    # Deliberately spoil this disposable fixture's own index; no auto repair.
    query('drop-owned-index-for-conflict','DROP INDEX yoke_native_element_revision_v1');query('create-wrong-owned-index','CREATE INDEX yoke_native_element_revision_v1 FOR (n:YokeNativeElement) ON (n.wrongScope)')
    denied=cli('wrong-schema-refused',['neo4j-publish','probe','--plan',path]+connect,success=False);assert 'schema_conflict' in denied
    still_wrong=query('wrong-schema-preserved','SHOW INDEXES YIELD name,properties WHERE name=$name RETURN properties',{'name':'yoke_native_element_revision_v1'})['data']['values'];assert still_wrong==[[['wrongScope']]]
"""
source=source.replace("    result={'actualServer':'Neo4j Community2026.09.0'",insert+"    result={'actualServer':'Neo4j Community2026.09.0'")
source=source.replace("'nativeTlsTested':True", "'nativeTlsTested':True,'allOriginalEntityEvidenceObservationJsonExact':True,'parallelDirectionsAndUnresolvedTargetsExact':True,'forcedTwoNativeUpdatesExactlyOneCommitted':True,'losingTransactionCreatedNoRowsAndCounterUnchanged':True,'historicalRecoveryAfterLaterActiveRevision':True,'actualCountConflictRolledBackAndReadRecoveryRefusedUntilRestored':True,'wrongExistingSchemaRefusedWithoutRepair':True")
source += "\nsource_after=inventory();(out/'source-after.json').write_text(json.dumps(source_after,indent=2)+'\\n',encoding='utf-8');assert source_before==source_after;assert binary_before==hashlib.sha256((primary/'target/debug/yoke.exe').read_bytes()).hexdigest();(out/'frozen-inputs.json').write_text(json.dumps({'inputs':len(source_before),'sourceUnchanged':True,'binaryUnchanged':True,'binarySha256':binary_before})+'\\n')\n"
assert 'race_barrier.wait(timeout=20)' in source and "historical=folder/'historical-pending'" in source and 'source_before=inventory()' in source
(temp/'ci-neo4j-native-final.py').write_text(source)
