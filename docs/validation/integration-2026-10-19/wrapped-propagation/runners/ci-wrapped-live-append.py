from pathlib import Path
temp=Path(__file__).parent
work=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach')
p=work/'native/tests/propagated_delete_live.rs'
s=p.read_text(encoding='utf-8');append=(temp/'ci-wrapped-live-test.rs').read_text(encoding='utf-8')
assert 'actual_wrapped_propagation_' not in s
p.write_text(s+append,encoding='utf-8')
