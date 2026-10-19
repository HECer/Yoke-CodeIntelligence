from pathlib import Path
temp=Path(__file__).parent;s=(temp/'ci-neo4j-baseline-r3.py').read_text(encoding='utf-8').replace('neo4j-publish/baseline-r3','neo4j-publish/schema-probe')
old="   scope='owned-scope-'";assert old in s
new='''   query('element-constraint','CREATE CONSTRAINT yoke_native_element_key_v1 IF NOT EXISTS FOR (n:YokeNativeElement) REQUIRE n.key IS UNIQUE')
   query('observation-constraint','CREATE CONSTRAINT yoke_native_observation_key_v1 IF NOT EXISTS FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() REQUIRE r.key IS UNIQUE')
   query('element-revision-index','CREATE INDEX yoke_native_element_revision_v1 IF NOT EXISTS FOR (n:YokeNativeElement) ON (n.scopeKey,n.revisionId)')
   query('observation-revision-index','CREATE INDEX yoke_native_observation_revision_v1 IF NOT EXISTS FOR ()-[r:YOKE_NATIVE_OBSERVATION]-() ON (r.scopeKey,r.revisionId)')
   schema=query('schema-receipt',"SHOW CONSTRAINTS YIELD name,type,entityType,labelsOrTypes,properties WHERE name STARTS WITH 'yoke_native_' RETURN name,type,entityType,labelsOrTypes,properties ORDER BY name")
   indexes=query('index-receipt',"SHOW INDEXES YIELD name,type,entityType,labelsOrTypes,properties,owningConstraint WHERE name STARTS WITH 'yoke_native_' RETURN name,type,entityType,labelsOrTypes,properties,owningConstraint ORDER BY name")
   assert len(schema['data']['values'])==3 and len(indexes['data']['values'])==5
   scope='owned-scope-'
'''
s=s.replace(old,new.rstrip())
(temp/'ci-neo4j-schema-probe.py').write_text(s,encoding='utf-8')
print('Prepared owned schema/relationship-uniqueness and range-index provider probe.')
