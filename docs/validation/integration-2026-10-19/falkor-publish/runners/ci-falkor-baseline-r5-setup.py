from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';source=(temp/'ci-falkor-vm-baseline-r4.py').read_text(encoding='utf-8-sig').replace("base/'vm-baseline-r4'","base/'vm-baseline-r5'")
block="""    query('map-parameter','CYPHER row={key:"owned",props:{value:"data"}} RETURN $row.key,$row.props.value')
    query('dynamic-map-keys','WITH {a:1,b:2} AS m RETURN all(k IN keys(m) WHERE m[k]>0)',None)
    query('count-subquery','RETURN COUNT { MATCH (n:Owned) } AS count',None)
    query('node-key-index','CREATE INDEX FOR (n:Owned) ON (n.id)')
    responses['constraint-create']=command(stream,['GRAPH.CONSTRAINT','CREATE',key,'UNIQUE','NODE','Owned','PROPERTIES','1','id'])
    for attempt in range(40):
     constraints=command(stream,['GRAPH.RO_QUERY',key,'CALL db.constraints()']);responses['constraints-'+str(attempt)]=constraints
     if 'OPERATIONAL' in json.dumps(constraints):break
     time.sleep(.05)
    else:raise AssertionError('Owned unique constraint did not become operational')
    query('duplicate-after-write','MATCH (a:Owned {id:"a"}) SET a.touched=1 CREATE (:Owned {id:"a"}) RETURN a.touched',False)
    untouched=query('constraint-rollback-state','MATCH (a:Owned {id:"a"}) RETURN a.touched');assert untouched[1]==[[None]],'Constraint failure did not roll back earlier mutation'
    query('arithmetic-after-write','MATCH (a:Owned {id:"a"}) SET a.touched=2 WITH a,0 AS denom SET a.guard=0/denom RETURN a.touched',False)
    untouched=query('arithmetic-rollback-state','MATCH (a:Owned {id:"a"}) RETURN a.touched');assert untouched[1]==[[None]],'Arithmetic failure did not roll back earlier mutation'
    query('nested-foreach','FOREACH (row IN [{id:3},{id:4}] | MERGE (n:Loop {id:row.id}) FOREACH (inner IN [1] | SET n.checked=inner)) RETURN 1')
    responses['watch-start']=command(stream,['WATCH',key])
    with socket.create_connection(('127.0.0.1',port),timeout=5) as other:
     other.settimeout(10);assert command(other,['AUTH','default',password])=='OK';responses['other-write']=command(other,['GRAPH.QUERY',key,'CREATE (:Watcher {id:1})'])
    assert command(stream,['MULTI'])=='OK';assert command(stream,['GRAPH.QUERY',key,'CREATE (:ShouldNotRun {id:1})'])=='QUEUED';responses['watched-exec']=command(stream,['EXEC']);assert responses['watched-exec'] is None
    query('watched-exec-no-write','MATCH (n:ShouldNotRun) RETURN count(n)')
"""
source=source.replace("    result={'actualImageTag'",block+"    result={'actualImageTag'")
source=source.replace("'unicodePasswordAuthentication':True", "'unicodePasswordAuthentication':True,'constraintPendingBecameOperational':True,'constraintFailureRolledBackEarlierMutation':True,'arithmeticFailureRolledBackEarlierMutation':True,'watchGraphMutationInvalidatesExec':True")
(temp/'ci-falkor-vm-baseline-r5.py').write_text(source,encoding='utf-8')
