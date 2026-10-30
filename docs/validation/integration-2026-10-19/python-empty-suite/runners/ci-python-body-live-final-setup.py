from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-python-body-check.py').read_text(encoding='utf-8')
assert source.count("phase=='live-r1'")==2
source=source.replace("phase=='live-r1'","phase=='live-final-r2'")
(temp/'ci-python-body-live-final.py').write_text(source,encoding='utf-8')
print('Final explicit Python AST runner prepared')
