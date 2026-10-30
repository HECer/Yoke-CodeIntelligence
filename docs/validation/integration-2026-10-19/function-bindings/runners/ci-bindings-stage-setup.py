from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-session-stage.py').read_text(encoding='utf-8')
s=s.replace('session-refactor','function-bindings').replace('ci-session-','ci-bindings-').replace('docs: record guarded native session refactoring validation','docs: record explicit function binding propagation validation')
(t/'ci-bindings-stage.py').write_text(s,encoding='utf-8')
print('Fresh exact-artifact staging prepared')
