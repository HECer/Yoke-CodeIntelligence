from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-provider-integrate.py').read_text(encoding='utf-8')
s=s.replace('mixed-meaning-provider','mixed-meaning-consumers').replace('final-r2','final')
s=s.replace("parent='9d6968637fdde5ac556af9eae8c07c2c07c2d3a7'","parent='db64d83581719aa98a4750c66de3454dbc43c8a0'")
s=s.replace("['groups']==18","['groups']==19")
start=s.index('paths=sorted(');end=s.index('\nchanged=',start)
s=s[:start]+"paths=sorted(['native/README.md','native/docs/mixed-meaning-provider.md','native/docs/mixed-meaning-queries.md','native/src/mixed_meaning_cli.rs','native/src/mixed_meaning_provider.rs','native/src/mixed_meaning_query.rs','native/tests/mixed_meaning_queries.rs'])"+s[end:]
s=s.replace('feat(native): build checked mixed meaning with durable accounting','feat(native): query source-checked mixed interpretation graphs')
s=s.replace('Merge explicit checked mixed meaning provider builds','Merge checked mixed interpretation graph queries')
s=s.replace("'defaultIntegration':11", "'defaultIntegration':8")
(t/'ci-consumers-integrate.py').write_text(s,encoding='utf-8')
s=(t/'ci-provider-integrated.py').read_text(encoding='utf-8').replace('mixed-meaning-provider','mixed-meaning-consumers').replace('START MIXED MEANING PROVIDER','START MIXED MEANING CONSUMERS')
(t/'ci-consumers-integrated.py').write_text(s,encoding='utf-8')
print('Fresh integration runners prepared; not executed')
