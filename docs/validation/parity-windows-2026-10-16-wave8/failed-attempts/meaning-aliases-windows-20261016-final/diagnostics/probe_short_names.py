"""Query real short names in disposable owned fixtures only."""
import ctypes as c
import json
from pathlib import Path
import tempfile

api = c.WinDLL("kernel32", use_last_error=True).GetShortPathNameW
api.argtypes = [c.c_wchar_p, c.c_wchar_p, c.c_uint32]
api.restype = c.c_uint32
results = []
for scratch in (Path("D:/Yoke-Test-Scratch"), Path(tempfile.gettempdir())):
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="short-name-probe-", dir=scratch) as raw:
        root = Path(raw).resolve()
        if not root.is_relative_to(scratch.resolve()):
            raise RuntimeError("Fixture escaped scratch directory")
        for leaf in ("LongIgnoredSourceDirectory", "node_modules"):
            path = root / leaf
            path.mkdir()
            buffer = c.create_unicode_buffer(32768)
            count = api(str(path), buffer, len(buffer))
            results.append(dict(drive=root.drive, leaf=leaf,
                                success=0 < count < len(buffer),
                                distinctAlias=Path(buffer.value).name != leaf))
print(json.dumps(results))
