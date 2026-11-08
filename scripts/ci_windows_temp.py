"""Select the same Windows CI temporary directory by its ordinary long path.

This changes only TEMP/TMP for subsequent CI steps. It creates no directory and
changes no system setting, ACL, privilege or product path-admission policy.
"""
import ctypes
import json
import os
from pathlib import Path
import re
import stat


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def ascii_root(value):
    require(isinstance(value, str) and 3 <= len(value) <= 4096
            and re.fullmatch(r"[A-Za-z]:[\\/][A-Za-z0-9_.\\/-]*", value),
            "CI fixture parent needs an ordinary ASCII long path")
    components = re.split(r"[\\/]", value[3:].rstrip("\\/"))
    require(components == [""] or all(part not in ("", ".", "..") for part in components),
            "Noncanonical CI fixture parent")
    return value


def directory_identity(path):
    value = path.stat(follow_symlinks=False)
    require(stat.S_ISDIR(value.st_mode) and not getattr(value, "st_file_attributes", 0) & 0x400,
            "CI temporary root must be an ordinary directory")
    require(value.st_ino != 0, "CI temporary directory identity unavailable")
    return value.st_dev, value.st_ino


def runtime_temp():
    # Match Rust1.94.1 std::env::temp_dir, including TMP-before-TEMP and SYSTEM
    # behavior, rather than selecting Python's potentially different root.
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    function = api.GetTempPath2W
    function.argtypes = [ctypes.c_uint32, ctypes.c_wchar_p]
    function.restype = ctypes.c_uint32
    buffer = ctypes.create_unicode_buffer(32768)
    length = function(len(buffer), buffer)
    require(0 < length < len(buffer) and buffer.value, "Native runtime temporary root unavailable or truncated")
    return Path(buffer.value)


def canonical_temp(original):
    original = Path(original)
    require(original.is_absolute(), "Explicit temporary root required")
    # A DOS short name is an alternate name of the same object. Reparse points
    # are a distinct case and are never normalized away by this setup step.
    for path in (original, *original.parents):
        require(not path.is_symlink() and not path.is_junction(), "Linked CI temporary ancestor refused")
    before = directory_identity(original)
    canonical = original.resolve(strict=True)
    text = ascii_root(str(canonical))
    for path in (canonical, *canonical.parents):
        require(not path.is_symlink() and not path.is_junction(), "Linked canonical temporary ancestor refused")
    require(directory_identity(canonical) == before and directory_identity(original) == before,
            "CI temporary directory identity changed")
    return {"original": str(original), "canonical": text, "sameDirectoryIdentityObserved": True}


def main():
    require(os.name == "nt" and os.environ.get("GITHUB_ACTIONS") == "true",
            "Canonical Windows fixture setup requires CI")
    result = canonical_temp(runtime_temp())
    output = Path(os.environ["GITHUB_ENV"])
    require(output.is_absolute() and not output.is_symlink() and not output.is_junction(),
            "Explicit ordinary CI environment file required")
    with output.open("a", encoding="utf-8", newline="\n") as target:
        target.write("\nTEMP=" + result["canonical"] + "\nTMP=" + result["canonical"] + "\n")
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
