from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-wrapped-check-v2.py').read_text(encoding='utf-8')
s=s.replace('wrapped-propagation','session-refactor').replace("['propagated_delete_contract']","['session_refactor']").replace('START WRAPPED PROPAGATION','START SESSION REFACTOR')
s=s.replace("['semantic_refactor.rs','semantic_propagated_delete.rs','semantic_propagated_delete_ast.rs','propagated_delete_contract.rs','propagated_delete_live.rs','propagated-delete-lsp.mjs','propagated-delete.md','semantic-refactor-intents.md','native/README.md']", "['project_session.rs','session_refactor.rs','session_mcp.rs','propagated-delete-lsp.mjs','agent-sessions.md','semantic-refactor-intents.md','native/README.md']")
s='\n'.join(line for line in s.split('\n') if not line.startswith("if phase=='"))
(t/'ci-session-check.py').write_text(s,encoding='utf-8')
print('Fresh session-refactor frozen-source runner prepared')
