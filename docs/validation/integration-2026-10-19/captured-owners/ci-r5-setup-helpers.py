from pathlib import Path
temp=Path.home()/'AppData/Local/Temp'
s=(temp/'ci-r2-integrated.py').read_text(encoding='utf-8')
s=s.replace('portable-projections','captured-owners').replace("dev=base/'r4'","dev=base/'r3'").replace("dev/'freeze.json'","dev/'summary.json'").replace('allNewCapabilityChecksPassed','allChecksPassed')
(temp/'ci-r5-integrated.py').write_text(s,encoding='utf-8',newline='\n')
print('Prepared integrated validation helper')
