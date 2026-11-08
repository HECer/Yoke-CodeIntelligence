from pathlib import Path
t=Path(__file__).parent
for name in ['ci-wrapped-commit.py','ci-wrapped-docs.py','ci-wrapped-preserve.py','ci-wrapped-pr.py']:
    p=t/name;s=p.read_text(encoding='utf-8')
    s=s.replace("'passed':585", "'passed':587").replace('585 passed','587 passed').replace('585/0/13','587/0/13')
    s=s.replace('468 / 0 / 7','469 / 0 / 7').replace('468/0/7','469/0/7').replace('28/0/0 propagated','29/0/0 propagated')
    s=s.replace("'defaultIntegration':8,'actualCompilerAndLanguageServer':1", "'unitAdmission':1,'defaultIntegration':9,'actualCompilerAndLanguageServer':1")
    s=s.replace('**nine new cases**: eight default integration and one actual gate.', '**eleven new cases**: one unit, nine default integration and one actual gate.')
    s=s.replace('Nine new distinct cases:8default integration+1explicit actual gate.', 'Eleven new distinct cases:1unit+9default integration+1explicit actual gate.')
    s=s.replace('Nine new cases:8default integration+1explicit actual gate.', 'Eleven new cases:1unit+9default integration+1explicit actual gate.')
    s=s.replace('retains8default+1actual new', 'retains1unit+9default+1actual new')
    s=s.replace('kind/order/mapped-byte', 'kind/nesting/field/order/mapped-byte')
    p.write_text(s,encoding='utf-8')
print('Future metadata updated for root-trivia and AST-work regressions')
