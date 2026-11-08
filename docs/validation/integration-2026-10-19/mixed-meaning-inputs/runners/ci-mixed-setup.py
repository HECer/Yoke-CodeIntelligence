from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-bindings-check.py').read_text(encoding='utf-8')
s=s.replace('function-bindings','mixed-meaning-inputs').replace('START FUNCTION BINDINGS','START MIXED MEANING INPUTS')
s=s.replace("['propagated_delete_contract']","['mixed_view_cli','mixed_pdf_view']")
s=s.replace("['semantic_propagated_delete.rs','semantic_propagated_delete_ast.rs','propagated_delete_contract.rs','session_refactor.rs','propagated-delete-lsp.mjs','propagated-delete.md','native/README.md']", "['mixed_view.rs','mixed_view_query.rs','mixed_meaning_input.rs','mixed_view_cli.rs','mixed_pdf_view.rs','mixed-views.md','native/README.md']")
s=s.replace("if phase=='baseline':command+=['function_binding_policy_removes_function_values_with_exact_original_entities']\n",'')
(t/'ci-mixed-check.py').write_text(s,encoding='utf-8')
print('Fresh mixed meaning input runner prepared')
