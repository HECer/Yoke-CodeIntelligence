from pathlib import Path
p=Path(r'C:\Users\Lomi\Documents\NN-Developed\Yoke-CI-r1-attach\native\src\semantic_propagated_delete_ast.rs')
assert not p.exists()
p.write_bytes((Path(__file__).parent/'ci-wrapped-ast.rs').read_bytes())
