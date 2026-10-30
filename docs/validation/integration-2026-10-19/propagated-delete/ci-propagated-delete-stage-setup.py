from pathlib import Path
base=Path('validation/integration-20261018');p=base/'verify-propagated-delete-staged.py';assert not p.exists();p.write_bytes((base/'verify-declaration-attachments-staged.py').read_bytes().replace(b'declaration-attachments',b'propagated-delete'))
