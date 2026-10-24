from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-r9-integrated.py').read_text(encoding='utf-8')
assert source.count('plan-feedback')==1
source=source.replace('plan-feedback','python-empty-suite').replace('START R9 INTEGRATED','START PYTHON SUITE INTEGRATED')
(temp/'ci-python-body-integrated.py').write_text(source,encoding='utf-8')
print('Python empty-suite integrated runner prepared')
