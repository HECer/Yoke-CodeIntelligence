from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-mixed-integrate.py').read_text(encoding='utf-8')
s=s.replace('mixed-meaning-inputs','mixed-meaning-claims').replace('work/r4-mixed-meaning-inputs-20261020','work/r4-mixed-meaning-claims-20261020')
s=s.replace("parent='d648ce1b483e1912715be1ed49c8d3ce79341a13'","parent='da45d2591cfb9f208e87d742eadc17a65fc8f50a'")
s=s.replace("['groups']==13","['groups']==14")
old="['native/README.md','native/docs/mixed-views.md','native/src/mixed_view.rs','native/src/mixed_view_query.rs','native/src/mixed_view_cli.rs','native/src/mixed_meaning_input.rs','native/tests/mixed_view_cli.rs','native/tests/mixed_pdf_view.rs']"
new="['native/README.md','native/docs/mixed-meaning-claims.md','native/src/meaning_graph.rs','native/src/mixed_view.rs','native/src/mixed_view_cli.rs','native/src/mixed_meaning_input.rs','native/src/mixed_meaning_claims.rs','native/tests/mixed_meaning_claims.rs']"
assert old in s;s=s.replace(old,new)
s=s.replace('feat(native): prepare checked mixed-source meaning inputs','feat(native): check source-bound mixed interpretation claims')
s=s.replace('Merge checked mixed-source meaning input preparation','Merge checked mixed-source interpretation claims')
s=s.replace("'defaultIntegration':7,'actualPdfWorkerIncludedInDefault':1","'defaultIntegration':5,'actualNativeCliIncludedInDefault':1")
(t/'ci-claims-integrate.py').write_text(s,encoding='utf-8')
s=(t/'ci-mixed-integrated.py').read_text(encoding='utf-8').replace('mixed-meaning-inputs','mixed-meaning-claims').replace('START MIXED MEANING INPUTS','START MIXED MEANING CLAIMS')
(t/'ci-claims-integrated.py').write_text(s,encoding='utf-8')
print('Fresh claims integration runners prepared')
