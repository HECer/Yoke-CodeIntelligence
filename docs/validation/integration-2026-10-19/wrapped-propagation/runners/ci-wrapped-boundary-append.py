from pathlib import Path
t=Path(__file__).parent;p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\tests\propagated_delete_contract.rs')
s=p.read_text(encoding='utf-8');assert 'fn wrapped_policy_preserves_surviving_statement_boundaries()' not in s
p.write_text(s+(t/'ci-wrapped-boundary-test.rs').read_text(encoding='utf-8'),encoding='utf-8')
