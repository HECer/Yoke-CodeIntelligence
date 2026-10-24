from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';raw=(temp/'ci-propagated-delete-r2.py').read_bytes();assert raw.count(b'propagated-delete-development/r2')==1;raw=raw.replace(b'propagated-delete-development/r2',b'propagated-delete-development/r3');(temp/'ci-propagated-delete-r3.py').write_bytes(raw)
