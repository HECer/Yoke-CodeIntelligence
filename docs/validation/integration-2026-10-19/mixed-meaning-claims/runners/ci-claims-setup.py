from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-mixed-check.py').read_text(encoding='utf-8')
s=s.replace('mixed-meaning-inputs','mixed-meaning-claims').replace('START MIXED MEANING INPUTS','START MIXED MEANING CLAIMS')
s=s.replace("['mixed_view_cli','mixed_pdf_view']","['mixed_meaning_claims','meaning_graph','mixed_view_cli','mixed_pdf_view']")
s=s.replace("['mixed_view.rs','mixed_view_query.rs','mixed_meaning_input.rs','mixed_view_cli.rs','mixed_pdf_view.rs','mixed-views.md','native/README.md']", "['mixed_view.rs','mixed_meaning_claims.rs','meaning_graph.rs','mixed_meaning_input.rs','mixed_view_cli.rs','mixed_pdf_view.rs','mixed-meaning-claims.md','native/README.md']")
(t/'ci-claims-check.py').write_text(s,encoding='utf-8')
print('Fresh checked-claims frozen runner prepared')
