from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-claims-check.py').read_text(encoding='utf-8')
s=s.replace('mixed-meaning-claims','mixed-meaning-provider').replace('START MIXED MEANING CLAIMS','START MIXED MEANING PROVIDER')
s=s.replace("['mixed_meaning_claims','meaning_graph','mixed_view_cli','mixed_pdf_view']","['mixed_meaning_claims','meaning_build']")
s=s.replace("'mixed_view.rs','mixed_meaning_claims.rs'","'mixed_view.rs','meaning_build.rs','mixed_meaning_provider.rs','mixed_meaning_claims.rs'")
s=s.replace("'meaning_build.rs','mixed_meaning_provider.rs'","'meaning_build.rs','retention.rs','mixed_meaning_provider.rs'")
s=s.replace("'retention.rs'","'retention.rs','native/src/main.rs','mixed_meaning_cli.rs'")
(t/'ci-provider-check.py').write_text(s,encoding='utf-8')
print('Fresh mixed provider frozen runner prepared')
