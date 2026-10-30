from pathlib import Path
import re
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\docs\mixed-meaning-composition.md')
s=p.read_text(encoding='utf-8')
s=re.sub(r'([a-zA-Z])(\d)',r'\1 \2',s)
p.write_text(s,encoding='utf-8')
