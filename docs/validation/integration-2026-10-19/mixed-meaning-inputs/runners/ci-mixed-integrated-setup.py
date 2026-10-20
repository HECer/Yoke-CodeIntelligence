from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-bindings-integrated.py').read_text(encoding='utf-8')
s=s.replace('function-bindings','mixed-meaning-inputs').replace('START FUNCTION BINDINGS','START MIXED MEANING INPUTS')
(t/'ci-mixed-integrated.py').write_text(s,encoding='utf-8')
print('Prepared integrated check runner')
