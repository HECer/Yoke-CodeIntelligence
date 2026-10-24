from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-claims-integrate.py').read_text(encoding='utf-8')
s=s.replace('mixed-meaning-claims','mixed-meaning-provider')
s=s.replace("parent='da45d2591cfb9f208e87d742eadc17a65fc8f50a'","parent='9d6968637fdde5ac556af9eae8c07c2c07c2d3a7'")
s=s.replace("['groups']==14","['groups']==18")
s=s.replace("'final/result.json'","'final-r2/result.json'").replace("'final/source-before.json'","'final-r2/source-before.json'").replace("'final/source-after.json'","'final-r2/source-after.json'")
old="['native/README.md','native/docs/mixed-meaning-provider.md','native/src/meaning_graph.rs','native/src/mixed_view.rs','native/src/mixed_view_cli.rs','native/src/mixed_meaning_input.rs','native/src/mixed_meaning_claims.rs','native/tests/mixed_meaning_claims.rs']"
new="['native/README.md','native/docs/mixed-meaning-claims.md','native/docs/mixed-views.md','native/docs/mixed-meaning-provider.md','native/src/main.rs','native/src/meaning_build.rs','native/src/mixed_view.rs','native/src/mixed_meaning_cli.rs','native/src/mixed_meaning_input.rs','native/src/mixed_meaning_claims.rs','native/src/mixed_meaning_provider.rs','native/src/retention.rs','native/tests/mixed_meaning_provider.rs']"
assert old in s;s=s.replace(old,new)
s=s.replace('feat(native): check source-bound mixed interpretation claims','feat(native): build checked mixed meaning with durable accounting')
s=s.replace('Merge checked mixed-source interpretation claims','Merge explicit checked mixed meaning provider builds')
s=s.replace("'defaultIntegration':5,'actualNativeCliIncludedInDefault':1","'defaultIntegration':11,'actualNativeCliIncludedInDefault':1")
(t/'ci-provider-integrate.py').write_text(s,encoding='utf-8')
s=(t/'ci-claims-integrated.py').read_text(encoding='utf-8').replace('mixed-meaning-claims','mixed-meaning-provider').replace('START MIXED MEANING CLAIMS','START MIXED MEANING PROVIDER')
(t/'ci-provider-integrated.py').write_text(s,encoding='utf-8')
print('Fresh mixed provider integration runners prepared')
