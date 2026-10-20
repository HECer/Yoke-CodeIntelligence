from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-members-check.py').read_text(encoding='utf-8')
source=source.replace('member-decorators','wrapped-propagation').replace("['member_decorators']","['propagated_delete_contract']").replace('START MEMBER DECORATORS','START WRAPPED PROPAGATION')
source=source.replace("['changes.rs','changes_symbol_delete.rs','changes_member_decorators.rs','member_decorators.rs','symbol_delete_ranges.rs','semantic-refactor-intents.md','symbol-deletion.md','native/README.md']", "['semantic_refactor.rs','semantic_propagated_delete.rs','propagated_delete_contract.rs','propagated_delete_live.rs','propagated-delete-lsp.mjs','propagated-delete.md','semantic-refactor-intents.md','native/README.md']")
source=source.replace("command+=['--','--test-threads=1']", "command+=['--','--test-threads=1']\nif phase=='baseline':command+=['wrapped_policy_removes_only_complete_parenthesized_await_and_void_statements']")
(temp/'ci-wrapped-check.py').write_text(source,encoding='utf-8')
for suffix in ['binary','integrated']:
    source=(temp/('ci-members-'+suffix+'.py')).read_text(encoding='utf-8')
    source=source.replace('member-decorators','wrapped-propagation').replace('START MEMBER DECORATORS','START WRAPPED PROPAGATION')
    (temp/('ci-wrapped-'+suffix+'.py')).write_text(source,encoding='utf-8')
print('Fresh frozen runner and probes prepared')
