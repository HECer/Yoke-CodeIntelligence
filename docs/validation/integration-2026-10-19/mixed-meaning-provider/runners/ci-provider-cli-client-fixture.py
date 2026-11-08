from pathlib import Path
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\tests\mixed_meaning_provider.rs')
s=p.read_text(encoding='utf-8');start=s.index('fn cli_build_requires_separate_trust_and_reads_replay_without_provider_configuration()')
a=s[:start];b=s[start:]
assert b.count('&f.engine.root')==2;b=b.replace('&f.engine.root','&root')
needle='    let run = |trust: bool| {'
assert needle in b;b=b.replace(needle,'    let root = f.engine.root.clone();\n'+needle,1)
needle='    let output = run(true);'
assert needle in b;b=b.replace(needle,'    drop(f.engine);\n'+needle,1)
p.write_text(a+b,encoding='utf-8',newline='\n')
