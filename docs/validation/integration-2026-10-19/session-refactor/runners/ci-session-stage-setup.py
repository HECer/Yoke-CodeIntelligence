from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-wrapped-stage.py').read_text(encoding='utf-8')
s=s.replace('wrapped-propagation','session-refactor').replace('ci-wrapped-','ci-session-').replace('docs: record explicit wrapped-call propagation validation','docs: record guarded native session refactoring validation')
(t/'ci-session-stage.py').write_text(s,encoding='utf-8')
print('Fresh exact-artifact staging prepared')
