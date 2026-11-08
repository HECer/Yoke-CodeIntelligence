from pathlib import Path
import py_compile
p=Path(__file__).parent;s=(p/'ci-falkor-native-live.py').read_text(encoding='utf-8').replace("out=base/'native-live-r1'","out=base/'native-final-r2'")
s=s.replace("proxy_events=[];drop_next=", "init_race={'key':None};proxy_events=[];drop_next=")
old="        upstream.sendall(wire);response,_=frame(upstream);"
new="""        if parts[0]==b'EXEC' and init_race['key'] is not None:
         key=init_race['key'];init_race['key']=None
         with socket.create_connection(('127.0.0.1',port),timeout=10) as interference:
          assert command(interference,['AUTH','default',password])=='OK';command(interference,['GRAPH.QUERY',key,'CREATE (:Foreign {preserve:7})'])
        upstream.sendall(wire);response,_=frame(upstream);"""
assert old in s;s=s.replace(old,new)
needle="     result={'actualImageTag':"
start=s.index(needle);s=s[:start]+(p/'ci-falkor-final-live-extra.txt').read_text(encoding='utf-8')+s[start:]
s=s.replace("'wrongSchemaRefusedWithoutRepair':True,", "'wrongSchemaRefusedWithoutRepair':True,'foreignExistingGraphRefusedWithoutChanges':True,'nativeWatchExecConflictKeepsOnlyForeignWriteAndPendingIntent':True,")
dest=p/'ci-falkor-native-final.py';dest.write_text(s,encoding='utf-8');py_compile.compile(str(dest),doraise=True)
s=(p/'ci-neo4j-native-final.py').read_text(encoding='utf-8').replace("neo4j-publish/native-final-r1","falkor-publish/neo4j-compatibility-r1");dest=p/'ci-falkor-neo4j-compatibility.py';dest.write_text(s,encoding='utf-8');py_compile.compile(str(dest),doraise=True);print('Prepared final Falkor live and Neo4j compatibility fixtures')
