from pathlib import Path
import difflib
p=Path(__file__).parent/'ci-provider-style-delta.py';s=p.read_text(encoding='utf-8');ns={}
exec(s[:s.index('assert s==new.decode()')],ns)
lines=list(difflib.unified_diff(ns['s'].splitlines(),ns['new'].decode().replace('\r\n','\n').splitlines(),n=2))
print('\n'.join(lines[:70]))
