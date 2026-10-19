from pathlib import Path
base=Path('validation/integration-20261018');raw=(base/'run-declaration-attachments-integrated.py').read_bytes();raw=raw.replace(b'declaration-attachments',b'propagated-delete').replace(b"else 'r3'",b"else 'r4'").replace(b"['r3','r4','r5']",b"['r4','r5','r6','r7','r8']");p=base/'run-propagated-delete-integrated.py';assert not p.exists();p.write_bytes(raw)
