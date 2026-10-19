from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';raw=(temp/'ci-propagated-delete-r4.py').read_bytes();assert raw.count(b'propagated-delete-development/r4')==1;raw=raw.replace(b'propagated-delete-development/r4',b'propagated-delete-development/r8');(temp/'ci-propagated-delete-r8.py').write_bytes(raw)
