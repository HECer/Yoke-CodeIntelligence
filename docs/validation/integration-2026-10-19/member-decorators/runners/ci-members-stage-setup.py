from pathlib import Path
temp=Path(__file__).parent
source=(temp/'ci-r9-stage.py').read_text(encoding='utf-8')
assert source.count('plan-feedback')==1
source=source.replace('plan-feedback','member-decorators').replace('ci-r9-stage.py','ci-members-stage.py').replace('ci-r9-pr.py','ci-members-pr.py')
source=source.replace('ci-r9-pathspec-','ci-members-pathspec-').replace('docs: record source-bound saved-plan outcome validation','docs: record checked class-method decorator ownership validation')
(temp/'ci-members-stage.py').write_text(source,encoding='utf-8')
print('Member-decorator evidence staging prepared')
