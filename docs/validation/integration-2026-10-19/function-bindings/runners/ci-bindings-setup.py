from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-session-check.py').read_text(encoding='utf-8')
s=s.replace('session-refactor','function-bindings').replace("['session_refactor']","['propagated_delete_contract']").replace('START SESSION REFACTOR','START FUNCTION BINDINGS')
old="['project_session.rs','protocol.rs','session_refactor.rs','session_mcp.rs','propagated-delete-lsp.mjs','agent-sessions.md','semantic-refactor-intents.md','native/README.md']"
new="['semantic_propagated_delete.rs','semantic_propagated_delete_ast.rs','propagated_delete_contract.rs','session_refactor.rs','propagated-delete-lsp.mjs','propagated-delete.md','native/README.md']"
assert old in s;s=s.replace(old,new)
s=s.replace("command+=['--','--test-threads=1']","command+=['--','--test-threads=1']\nif phase=='baseline':command+=['function_binding_policy_removes_function_values_with_exact_original_entities']")
(t/'ci-bindings-check.py').write_text(s,encoding='utf-8')
print('Fresh function-binding frozen-source runner prepared')
