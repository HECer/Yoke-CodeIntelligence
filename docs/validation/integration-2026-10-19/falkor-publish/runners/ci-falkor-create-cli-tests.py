from pathlib import Path
wt=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
for src,dest in [('native/src/neo4j_publish_cli.rs','native/src/falkor_publish_cli.rs'),('native/tests/neo4j_publish_cli.rs','native/tests/falkor_publish_cli.rs'),('native/src/neo4j_publish_journal_tests.rs','native/src/falkor_publish_journal_tests.rs')]:
 s=(wt/src).read_text(encoding='utf-8').replace('neo4j','falkor').replace('Neo4j','FalkorDB').replace('allow_loopback_http','allow_loopback_plaintext')
 s=s.replace('http://127.0.0.1:7474/','redis://127.0.0.1:6379/').replace('127.0.0.1:7474','127.0.0.1:6379').replace('"database":"falkor",','').replace('"user":"falkor"','"user":"default"')
 if 'journal_tests' in src:
  s=s.replace('Operation::ScopeConstraint','Operation::Initialize').replace('Operation::Namespace','Operation::Initialize').replace('completed(), 6','completed(), 9').replace('\n        6\n','\n        9\n')
 (wt/dest).write_text(s,encoding='utf-8')
p=wt/'native/src/main.rs';s=p.read_text(encoding='utf-8')
assert s.count('mod neo4j_publish_cli;')==1
s=s.replace('mod neo4j_publish_cli;','mod neo4j_publish_cli;\nmod falkor_publish_cli;')
s=s.replace('    Neo4jPublish(Box<neo4j_publish_cli::Arguments>),','    Neo4jPublish(Box<neo4j_publish_cli::Arguments>),\n    FalkorPublish(Box<falkor_publish_cli::Arguments>),')
s=s.replace('        ExtensionOperation::Neo4jPublish(_) => bail!("Neo4j publication host dispatch error"),','        ExtensionOperation::Neo4jPublish(_) => bail!("Neo4j publication host dispatch error"),\n        ExtensionOperation::FalkorPublish(_) => bail!("FalkorDB publication host dispatch error"),')
start=s.index('    if let Operation::Extensions(extension) = &operation\n        && let ExtensionOperation::Neo4jPublish(args)')
end=s.index('    if let Operation::Extensions(extension)',start+10)
block=s[start:end].replace('Neo4jPublish','FalkorPublish').replace('Neo4j publication','FalkorDB publication').replace('neo4j_publish_cli','falkor_publish_cli')
s=s[:end]+block+s[end:];p.write_text(s,encoding='utf-8')
p=wt/'native/src/falkor_publish_journal.rs';s=p.read_text(encoding='utf-8');s=s.replace('pub(super) fn now()', '#[cfg(all(test, windows))]\n#[path="falkor_publish_journal_tests.rs"]\nmod tests;\npub(super) fn now()');p.write_text(s,encoding='utf-8')
p=wt/'native/src/falkor_publish_program.rs';s=p.read_text(encoding='utf-8').replace('include_str!("falkor_publish_client.rs"),','include_str!("falkor_publish_client.rs"),\n        include_str!("falkor_publish_cli.rs"),');p.write_text(s,encoding='utf-8')
