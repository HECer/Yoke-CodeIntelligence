from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';raw=(temp/'ci-propagated-delete-reference-red-r5.py').read_bytes();assert raw.count(b'propagated-delete-development/reference-red-r5')==1;raw=raw.replace(b'propagated-delete-development/reference-red-r5',b'propagated-delete-development/binding-red-r7');(temp/'ci-propagated-delete-binding-red-r7.py').write_bytes(raw)
