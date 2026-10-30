from pathlib import Path
source=(Path.home()/'AppData/Local/Temp/ci-neo4j-baseline-r3.py').read_text()
source=source.replace("neo4j-publish/baseline-r3", "neo4j-publish/query-live-r1")
start=source.index("   marker='owned-'")
end=source.index("  finally:\n   if child.poll()",start)
replacement='''   target={'baseUrl':f'http://127.0.0.1:{http}/','address':f'127.0.0.1:{http}','database':'neo4j','namespace':'owned graph '+chr(0x1f4a1),'user':'neo4j','useSystemRoots':False}
   target_path=out/'public-target.json';target_path.write_text(json.dumps(target),encoding='utf-8')
   example=primary/'target/debug/examples/neo4j_owned_probe.exe'
   generated=run([example,target_path,out/'generated'],timeout=40)
   (out/'generator.log').write_bytes(generated.stdout+generated.stderr);assert generated.returncode==0
   bootstrap=json.loads((out/'generated/bootstrap-requests.json').read_bytes());publication=json.loads((out/'generated/publish-requests.json').read_bytes())[0]
   def submit(name,request,success=True):
    assert request['statement'].startswith('CYPHER 5 ')
    return query(name,request['statement'][9:],request['parameters'],success)
   for i,request in enumerate(bootstrap):submit('bootstrap-'+str(i),request)
   first=submit('publish-first',publication);repeat=submit('publish-repeat',publication)
   assert first['data']==repeat['data']
   receipt=first['data']['values'][0][0];params=publication['parameters'];assert receipt['observationCount']==len(params['observations'])
   def frozen_state(label):
    return query(label,'MATCH (s:YokeNativeScope {key:$key}) RETURN s.activeRevision,s.lockCounter,COUNT { MATCH (n:YokeNativeElement {scopeKey:$key}) },COUNT { MATCH ()-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$key}]->() }',{'key':params['scopeKey']})['data']['values']
   before=frozen_state('before-denials')
   stale=json.loads(json.dumps(publication));stale['parameters']['expectedRevision']='wrong';stale['parameters']['revisionId']='0'*64
   submit('stale-denied',stale,False);assert frozen_state('after-stale')==before
   altered=json.loads(json.dumps(publication));altered['parameters']['entities'][0]['props']['label']='altered server row'
   submit('immutable-row-denied',altered,False);assert frozen_state('after-row-denied')==before
   read=query('roundtrip-elements','MATCH (n:YokeNativeElement {scopeKey:$key}) RETURN n.key,n.rowHash,n.rowJson ORDER BY n.key',{'key':params['scopeKey']})
   expected={r['key']:(r['rowHash'],r['props']['rowJson']) for category in ['entities','evidence','unresolved'] for r in params[category]};expected[params['revision']['key']]=(params['revision']['rowHash'],params['revision']['props']['rowJson'])
   assert {r[0]:(r[1],r[2]) for r in read['data']['values']}==expected
   relationships=query('roundtrip-observations','MATCH (a:YokeNativeElement)-[r:YOKE_NATIVE_OBSERVATION {scopeKey:$key}]->(b:YokeNativeElement) RETURN r.key,a.key,b.key,r.rowHash,r.rowJson ORDER BY r.key',{'key':params['scopeKey']})
   assert {r[0]:r[1:] for r in relationships['data']['values']}=={r['key']:[r['sourceKey'],r['targetKey'],r['rowHash'],r['props']['rowJson']] for r in params['observations']}
   assert len(params['unresolved'])>0
   parallel={}
   for r in params['observations']:
    pair=(r['sourceKey'],r['targetKey']);parallel[pair]=parallel.get(pair,0)+1
   assert max(parallel.values())>=2
   discovery=query('fleet-disabled','SHOW SETTINGS YIELD name,value WHERE name=$name RETURN value',{'name':'server.fleet_discovery.enabled'});assert discovery['data']['values']==[['false']]
   result={'actualServer':'Neo4j Community2026.09.0','nativeGeneratedFixedCypher':True,'nativeTransportTested':False,'firstPublicationAndReplayIdentical':True,'staleTransactionRolledBack':True,'immutableRowConflictRolledBack':True,'allElementRowJsonAndHashesExact':True,'allObservationDirectionsParallelKeysAndJsonExact':True,'unresolvedTargetsExplicit':True,'entityCount':len(params['entities']),'evidenceCount':len(params['evidence']),'observationCount':len(params['observations']),'unresolvedCount':len(params['unresolved']),'fleetDiscoveryConfiguredDisabled':True,'ownedPid':child.pid,'listenerPidVerified':True,'java':'JBR25.0.3','vendorJvmQualificationClaimed':False}
'''
source=source[:start]+replacement+source[end:]
(Path.home()/'AppData/Local/Temp/ci-neo4j-query-live.py').write_text(source)
