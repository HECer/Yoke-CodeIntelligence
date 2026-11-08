from pathlib import Path
temp=Path(__file__).parent
s=(temp/'ci-members-stage.py').read_text(encoding='utf-8')
s=s.replace('member-decorators','wrapped-propagation').replace('ci-members-','ci-wrapped-').replace('docs: record checked class-method decorator ownership validation','docs: record explicit wrapped-call propagation validation')
(temp/'ci-wrapped-stage.py').write_text(s,encoding='utf-8')
print('Fresh exact-artifact staging prepared')
