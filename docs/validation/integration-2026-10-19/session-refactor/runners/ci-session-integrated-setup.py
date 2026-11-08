from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-wrapped-integrated.py').read_text(encoding='utf-8')
assert 'wrapped-propagation' in s
(t/'ci-session-integrated.py').write_text(s.replace('wrapped-propagation','session-refactor').replace('START WRAPPED PROPAGATION','START SESSION REFACTOR'),encoding='utf-8')
print('Fresh integrated runner prepared')
