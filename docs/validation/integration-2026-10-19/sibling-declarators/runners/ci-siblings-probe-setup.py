from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-python-body-binary.py').read_text(encoding='utf-8')
assert source.count('python-empty-suite')==1
(temp/'ci-siblings-binary.py').write_text(source.replace('python-empty-suite','sibling-declarators'),encoding='utf-8')
source=(temp/'ci-python-body-integrated.py').read_text(encoding='utf-8')
assert source.count('python-empty-suite')==1
(temp/'ci-siblings-integrated.py').write_text(source.replace('python-empty-suite','sibling-declarators').replace('START PYTHON SUITE INTEGRATED','START SIBLING DECLARATORS INTEGRATED'),encoding='utf-8')
print('Sibling binary probe and integrated runner prepared')
