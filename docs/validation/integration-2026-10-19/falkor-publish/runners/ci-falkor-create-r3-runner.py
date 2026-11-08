from pathlib import Path
p=Path(__file__).parent
s=(p/'ci-falkor-plan-check-r2.py').read_text(encoding='utf-8').replace('/plan-r2','/client-r3')
s=s.replace("commands=[('test',", "commands=[('unit',[cargo,'test',*common,'--lib','falkor','--','--test-threads=1']),('test',")
s=s.replace("'--test','neo4j_publish','--'", "'--test','neo4j_publish','--test','falkor_publish_cli','--test','neo4j_publish_cli','--'")
s=s.replace("['native/src/lib.rs','native/src/neo4j_publish.rs']", "['native/src/lib.rs','native/src/neo4j_publish.rs','native/src/main.rs','native/src/host_credentials.rs']")
(p/'ci-falkor-client-check-r3.py').write_text(s,encoding='utf-8')
