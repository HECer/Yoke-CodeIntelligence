from pathlib import Path
p=Path(__file__).parent;s=(p/'ci-falkor-integrated.py').read_text(encoding='utf-8').replace('falkor-publish/integrated','codex-host/integrated');(p/'ci-codex-integrated.py').write_text(s,encoding='utf-8')
