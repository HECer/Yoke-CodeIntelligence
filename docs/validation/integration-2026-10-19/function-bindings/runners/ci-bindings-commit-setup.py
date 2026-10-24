from pathlib import Path
t=Path(__file__).parent
s=(t/'ci-session-commit.py').read_text(encoding='utf-8')
s=s.replace('session-refactor','function-bindings').replace('0b9cdecb4e91d2651ce660959440d00598c9e7f1','2bc4df49423332497a14118cbd45d72c32ecb352')
s=s.replace('work/r6-function-bindings-20261020','work/r1-function-bindings-20261020')
s=s.replace("{'passed':1,'failed':0,'ignored':0}","{'passed':2,'failed':0,'ignored':0}")
s=s.replace("{'native/README.md','native/docs/agent-sessions.md','native/src/project_session.rs','native/src/protocol.rs','native/tests/session_refactor.rs'}", "{'native/README.md','native/docs/propagated-delete.md','native/src/semantic_propagated_delete.rs','native/tests/fixtures/propagated-delete-lsp.mjs','native/tests/propagated_delete_contract.rs','native/tests/session_refactor.rs'}")
s=s.replace('feat(native): expose guarded semantic plans through sessions','feat(native): propagate checked function-valued binding deletion')
s=s.replace('Merge guarded native session refactoring workflow','Merge explicit function binding propagation policy')
s=s.replace("{'defaultIntegration':9,'actualNativeCliCompilerAndLanguageServer':1}","{'defaultIntegration':10,'actualFunctionBindingsCli':1}")
assert 'work/r1-function-bindings-20261020' in s and 'work/r6-' not in s
assert "'native/docs/propagated-delete.md'" in s and "'native/docs/agent-sessions.md'" not in s
(t/'ci-bindings-commit.py').write_text(s,encoding='utf-8')
s=(t/'ci-session-integrated.py').read_text(encoding='utf-8')
(t/'ci-bindings-integrated.py').write_text(s.replace('session-refactor','function-bindings').replace('START SESSION REFACTOR','START FUNCTION BINDINGS'),encoding='utf-8')
print('Fresh function-binding integration and commit helpers prepared')
