from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-consumers-integrate.py').read_text(encoding='utf-8').replace('mixed-meaning-consumers','mixed-meaning-composition').replace('final-r2','final')
s=s.replace("parent='db64d83581719aa98a4750c66de3454dbc43c8a0'","parent='87be1a19ae1556099352fc7400034fdd109acae6'")
s=s.replace("['groups']==19","['groups']==20")
start=s.index('paths=sorted(');end=s.index('\nchanged=',start)
s=s[:start]+"paths=sorted(['native/README.md','native/docs/mixed-meaning-queries.md','native/docs/mixed-meaning-composition.md','native/src/mixed_meaning_cli.rs','native/src/mixed_meaning_provider.rs','native/src/mixed_meaning_query.rs','native/src/mixed_meaning_composition.rs','native/tests/mixed_meaning_composition.rs'])"+s[end:]
s=s.replace('feat(native): query source-checked mixed interpretation graphs','feat(native): compose checked mixed interpretation graph selections')
s=s.replace('Merge checked mixed interpretation graph queries','Merge checked mixed interpretation graph composition')
(t/'ci-composition-integrate.py').write_text(s,encoding='utf-8')
s=(t/'ci-consumers-integrated.py').read_text(encoding='utf-8').replace('mixed-meaning-consumers','mixed-meaning-composition').replace('START MIXED MEANING CONSUMERS','START MIXED MEANING COMPOSITION')
(t/'ci-composition-integrated.py').write_text(s,encoding='utf-8')
print('Prepared fresh composition integration helpers; not executed')
