from pathlib import Path
temp=Path(__file__).parent
s=(temp/'ci-wrapped-check.py').read_text(encoding='utf-8')
assert 'semantic_propagated_delete_ast.rs' not in s
s=s.replace("'semantic_propagated_delete.rs',", "'semantic_propagated_delete.rs','semantic_propagated_delete_ast.rs',")
s=s.replace("if phase=='baseline':", "if phase=='asi-baseline':command+=['wrapped_policy_preserves_surviving_statement_boundaries']\nif phase=='baseline':")
(temp/'ci-wrapped-check-v2.py').write_text(s,encoding='utf-8')
print('Separate runner prepared for surviving-syntax follow-up')
