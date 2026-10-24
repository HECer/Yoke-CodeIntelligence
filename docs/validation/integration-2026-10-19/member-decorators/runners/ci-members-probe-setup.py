from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-siblings-binary.py').read_text(encoding='utf-8')
assert source.count('sibling-declarators')==1
(temp/'ci-members-binary.py').write_text(source.replace('sibling-declarators','member-decorators'),encoding='utf-8')
source=(temp/'ci-siblings-integrated.py').read_text(encoding='utf-8')
assert source.count('sibling-declarators')==1
(temp/'ci-members-integrated.py').write_text(source.replace('sibling-declarators','member-decorators').replace('START SIBLING DECLARATORS INTEGRATED','START MEMBER DECORATORS INTEGRATED'),encoding='utf-8')
print('Member-decorator binary probe and integrated runner prepared')
