from pathlib import Path
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=work/'native/src/protocol.rs';text=p.read_text(encoding='utf-8')
old='"emptyPythonBody":{"enum":["reject","pass"]}'
assert text.count(old)==1
text=text.replace(old,old+',"siblingDeclarators":{"enum":["reject","preserve"]}')
p.write_text(text,encoding='utf-8')
print('Closed sibling-declarators schema added')
