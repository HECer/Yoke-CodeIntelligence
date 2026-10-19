from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';raw=(temp/'ci-declaration-attachments-r2.py').read_bytes();assert raw.count(b'declaration-attachments-development/r2')==1;raw=raw.replace(b'declaration-attachments-development/r2',b'declaration-attachments-development/r3');(temp/'ci-declaration-attachments-r3.py').write_bytes(raw)
