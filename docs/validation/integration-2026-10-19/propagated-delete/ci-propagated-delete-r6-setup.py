from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';raw=(temp/'ci-propagated-delete-r4.py').read_bytes();assert raw.count(b'propagated-delete-development/r4')==1;raw=raw.replace(b'propagated-delete-development/r4',b'propagated-delete-development/r6');(temp/'ci-propagated-delete-r6.py').write_bytes(raw)
