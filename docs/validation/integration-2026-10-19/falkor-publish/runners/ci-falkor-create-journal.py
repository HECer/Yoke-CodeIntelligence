from pathlib import Path
wt=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
s=(wt/'native/src/neo4j_publish_journal.rs').read_text(encoding='utf-8')
s=s.replace('neo4j','falkor').replace('Neo4j','FalkorDB')
s=s.replace('events.len() < 16','events.len() < 64')
s=s.replace('#[cfg(all(test, windows))]\n#[path = "falkor_publish_journal_tests.rs"]\nmod tests;\n','')
(wt/'native/src/falkor_publish_journal.rs').write_text(s,encoding='utf-8')
