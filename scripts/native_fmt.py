"""Check every Cargo target with short, check-only rustfmt commands.

Cargo metadata is queried offline without builds. Local path dependencies are
included, as with cargo fmt --all. Rustfmt retains its ordinary module recursion
and configuration discovery; no formatter configuration is overridden.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

MAX_METADATA_BYTES = 16_000_000
MAX_MANIFESTS = 128
MAX_ROOTS = 16_384
EDITIONS = {"2015", "2018", "2021", "2024"}


class FormatError(Exception):
    def __init__(self, message, returncode=2):
        super().__init__(message)
        self.returncode = returncode if returncode > 0 else 1


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise FormatError("Duplicate Cargo metadata key")
        result[key] = value
    return result


def cargo_metadata(manifest):
    command = ["cargo", "metadata", "--offline", "--locked", "--no-deps",
               "--format-version", "1", "--manifest-path", str(manifest)]
    # Spill to a private temporary file; admit the byte length before JSON decode.
    # This is an application inspection bound, not an OS disk quota.
    with tempfile.TemporaryFile() as output:
        child = subprocess.run(command, stdout=output, timeout=120)
        if child.returncode:
            raise FormatError("Cargo metadata failed; formatting was not started", child.returncode)
        if output.tell() > MAX_METADATA_BYTES:
            raise FormatError("Cargo metadata exceeds inspection capacity")
        output.seek(0)
        try:
            return json.loads(output.read(), object_pairs_hook=unique_object)
        except (ValueError, UnicodeError, RecursionError) as error:
            raise FormatError("Invalid Cargo metadata") from error


def existing_file(value):
    if not isinstance(value, str) or not Path(value).is_absolute():
        raise FormatError("Cargo metadata requires absolute file paths")
    try:
        path = Path(value).resolve(strict=True)
    except OSError as error:
        raise FormatError("Cargo metadata references an unavailable file") from error
    if not path.is_file():
        raise FormatError("Cargo metadata requires existing ordinary files")
    return path


def format_roots(manifest):
    manifest = Path(manifest).resolve(strict=True)
    if manifest.name != "Cargo.toml" or not manifest.is_file():
        raise FormatError("Expected an existing Cargo.toml")
    pending = [manifest]
    visited = set()
    covered = set()
    roots = {}
    while pending:
        selected = pending.pop()
        if selected in visited or selected in covered:
            continue
        if len(visited) >= MAX_MANIFESTS:
            raise FormatError("Cargo manifest inspection capacity exceeded")
        visited.add(selected)
        data = cargo_metadata(selected)
        if not isinstance(data, dict) or not isinstance(data.get("packages"), list) or not data["packages"]:
            raise FormatError("Cargo metadata has no packages")
        for package in data["packages"]:
            if not isinstance(package, dict):
                raise FormatError("Invalid Cargo package metadata")
            covered.add(existing_file(package.get("manifest_path")))
            targets = package.get("targets")
            dependencies = package.get("dependencies", [])
            if not isinstance(targets, list) or not targets or not isinstance(dependencies, list):
                raise FormatError("Invalid Cargo target/dependency inventory")
            for target in targets:
                if (not isinstance(target, dict) or not isinstance(target.get("edition"), str)
                        or target["edition"] not in EDITIONS):
                    raise FormatError("Unsupported Cargo target edition")
                path = existing_file(target.get("src_path"))
                edition = target["edition"]
                if path in roots and roots[path] != edition:
                    raise FormatError("Conflicting editions for one source root")
                roots[path] = edition
                if len(roots) > MAX_ROOTS:
                    raise FormatError("Cargo target inspection capacity exceeded")
            for dependency in dependencies:
                if not isinstance(dependency, dict):
                    raise FormatError("Invalid Cargo dependency metadata")
                path = dependency.get("path")
                if path is not None:
                    if not isinstance(path, str) or not Path(path).is_absolute():
                        raise FormatError("Invalid local dependency path")
                    dependency_manifest = existing_file(str(Path(path) / "Cargo.toml"))
                    if dependency_manifest not in visited and dependency_manifest not in covered:
                        pending.append(dependency_manifest)
                        if len(pending) > MAX_MANIFESTS:
                            raise FormatError("Cargo manifest queue capacity exceeded")
    if not roots:
        raise FormatError("Cargo metadata has no format roots")
    return sorted(roots.items())


def check_roots(roots):
    formatter = os.environ.get("RUSTFMT", "rustfmt")
    failures = []
    for number, (path, edition) in enumerate(roots, 1):
        print(f"Native format root {number}/{len(roots)} [{edition}]: {path}", flush=True)
        child = subprocess.run([formatter, str(path), "--edition", edition, "--check"], timeout=120)
        if child.returncode:
            failures.append(child.returncode if child.returncode > 0 else 1)
    print(f"Native format check: {len(roots)} roots, {len(failures)} failed", flush=True)
    return failures[0] if failures else 0


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="check all roots without modifying source")
    mode.add_argument("--list", action="store_true", help="list the complete admitted roots as JSON")
    parser.add_argument("--manifest-path", type=Path, default=Path.cwd() / "Cargo.toml")
    options = parser.parse_args(arguments)
    try:
        roots = format_roots(options.manifest_path)
        if options.list:
            print(json.dumps([{"path": str(path), "edition": edition} for path, edition in roots], ensure_ascii=True))
            return 0
        return check_roots(roots)
    except FormatError as error:
        print(f"Native format check incomplete: {error}", file=sys.stderr)
        return error.returncode
    except (OSError, subprocess.TimeoutExpired, UnicodeError):
        print("Native format check incomplete: unavailable input/tool or process timeout", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    sys.exit(main())
