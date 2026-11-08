from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';raw=(temp/'ci-propagated-delete-red.py').read_bytes();assert raw.count(b'propagated-delete-development/red-r1')==1;raw=raw.replace(b'propagated-delete-development/red-r1',b'propagated-delete-development/r2');(temp/'ci-propagated-delete-r2.py').write_bytes(raw)
