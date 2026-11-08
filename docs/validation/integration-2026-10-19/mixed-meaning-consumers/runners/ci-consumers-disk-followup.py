from pathlib import Path
t=Path(__file__).parent
p=t/'ci-consumers-integrate.py';s=p.read_text(encoding='utf-8').replace("'final/", "'final-r2/");p.write_text(s,encoding='utf-8')
p=t/'ci-consumers-record.py';s=p.read_text(encoding='utf-8').replace("read('final/result.json')", "read('final-r2/result.json')").replace("['feature-r1','final']", "['feature-r1','final','final-r2']")
s=s.replace("Initial17/1 is retained:","Initial17/1 and the first final run's disk-full linker refusal are retained:\n+the latter executes no complete native acceptance and does not change source.\n+The initial17/1 run's".replace('\n+','\n'))
s=s.replace("for file in sorted(temp.glob('ci-consumers-*.py')):shutil.copyfile(file,out/'runners'/file.name)","for file in sorted(temp.glob('ci-consumers-*')):\n    if file.suffix in ['.py','.ps1']:shutil.copyfile(file,out/'runners'/file.name)")
s=s.replace('Initial17/1 wrong expected refusal code is retained. Scoped nonadditive evidence;', 'Initial17/1 wrong expected refusal code and disk-full linker failure are retained.\n+Scoped nonadditive evidence;'.replace('\n+','\n'))
p.write_text(s,encoding='utf-8')
print('Follow-up evidence records first failed final run separately')
