"""Build a portable Rust archive with the locked dependency license texts.

No indexed workspace/state, language server, Node/Python runtime or upstream
application is bundled. Missing license texts fail unless fetching their exact
upstream revision is explicitly enabled.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import zipfile
from urllib.parse import quote


def command(*args, timeout=900):
    return subprocess.run(args, check=True, capture_output=True, text=True, timeout=timeout).stdout


def upstream_path(value, allow_empty=False):
    if allow_empty and value == "":
        return PurePosixPath()
    if (not isinstance(value, str) or not value or len(value) > 4096
            or "\\" in value or any(ord(char) < 32 for char in value)
            or any(part in ("", ".", "..") for part in value.split("/"))):
        raise RuntimeError("Invalid upstream repository path")
    return PurePosixPath(value)


def license_name(name):
    # Some pinned crates use MIT-LICENSE / Apache-LICENSE rather than LICENSE-MIT.
    # Keep the additional forms closed; executable/backup lookalikes are not notices.
    return bool(re.match(r"^(LICENSE|LICENCE|COPYING|UNLICENSE|NOTICE|COPYRIGHT)", name, re.I | re.ASCII)
                or re.fullmatch(r"(?:MIT|APACHE(?:-2\.0)?)[-_]LICEN[CS]E(?:\.(?:txt|md))?", name, re.I | re.ASCII))


def github_licenses(package, root):
    repository = package.get("repository") or ""
    match = re.fullmatch(r"https://github\.com/([^/]+/[^/]+?)(?:\.git)?/?", repository)
    if not match:
        raise RuntimeError(f"No supported pinned license source for {package['name']}")
    vcs = json.loads((root / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    revision = vcs["git"]["sha1"]
    if not re.fullmatch(r"[a-f0-9]{40}", revision):
        raise RuntimeError("Invalid upstream revision")
    slug = match.group(1)
    crate_path = upstream_path(vcs.get("path_in_vcs", ""), allow_empty=True)
    ancestors = set(crate_path.parents) | {crate_path}
    tree = json.loads(command("gh", "api", f"repos/{slug}/git/trees/{revision}?recursive=1", timeout=60))
    if tree.get("truncated", True):
        raise RuntimeError(f"Upstream license tree is truncated for {package['name']} at {revision}")
    if not isinstance(tree.get("tree"), list) or len(tree["tree"]) > 100_000:
        raise RuntimeError("Upstream repository tree exceeds entry budget")
    candidates = []
    seen = set()
    for entry in tree["tree"]:
        if entry["type"] != "blob":
            continue
        path = upstream_path(entry["path"])
        if not license_name(path.name):
            continue
        # Crate-local notices and ancestor-level repository licenses apply to
        # this published source scope. Unrelated sibling crate texts do not.
        inside = path.parts[:len(crate_path.parts)] == crate_path.parts
        if not inside and path.parent not in ancestors:
            continue
        if path.as_posix() in seen:
            raise RuntimeError("Duplicate upstream license path")
        seen.add(path.as_posix())
        if entry.get("mode") not in ("100644", "100755"):
            raise RuntimeError("Linked or unsupported upstream license input")
        if not re.fullmatch(r"[a-f0-9]{40}", entry["sha"]):
            raise RuntimeError("Invalid upstream license object identity")
        size = entry.get("size")
        if not isinstance(size, int) or not 0 < size <= 1_000_000:
            raise RuntimeError("Upstream license exceeds byte budget or is empty")
        candidates.append(entry)
    if len(candidates) > 64 or sum(entry["size"] for entry in candidates) > 8_000_000:
        raise RuntimeError("Upstream license inventory exceeds budget")
    licenses = []
    for entry in sorted(candidates, key=lambda item: item["path"]):
        blob = json.loads(command("gh", "api", f"repos/{slug}/git/blobs/{entry['sha']}", timeout=60))
        if blob["encoding"] != "base64":
            raise RuntimeError("Unsupported GitHub blob encoding")
        data = base64.b64decode("".join(blob["content"].split()), validate=True)
        git_id = hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data).hexdigest()
        if (len(data) != entry["size"] or blob.get("size") != len(data)
                or blob.get("sha") != entry["sha"] or git_id != entry["sha"]):
            raise RuntimeError("Upstream license blob identity/hash or size mismatch")
        text = data.decode("utf-8")
        if "\0" in text or text.startswith("version https://git-lfs.github.com/spec/v1"):
            raise RuntimeError("Unsupported binary or indirect upstream license text")
        licenses.append((entry["path"], data, f"https://github.com/{slug}/blob/{revision}/{quote(entry['path'])}"))
    if not licenses:
        raise RuntimeError(
            f"No upstream license text for {package['name']} {package.get('version', '')} at pinned "
            f"revision {revision} ({repository}); metadata declares {package.get('license')!r} "
            "but is not a substitute for missing upstream notice text"
        )
    return licenses


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--fetch-missing-licenses", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    if not args.skip_build:
        subprocess.run(["cargo", "build", "--workspace", "--release", "--locked", "-j", "2"], cwd=repo, check=True)
    host = next(line.split(": ", 1)[1] for line in command("rustc", "-vV").splitlines() if line.startswith("host: "))
    metadata = json.loads(command("cargo", "metadata", "--manifest-path", str(repo / "Cargo.toml"), "--locked", "--offline", "--format-version", "1", "--filter-platform", host))
    root_id = next(package["id"] for package in metadata["packages"] if package["name"] == "yoke-code-intelligence")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    included, pending = set(), [root_id]
    while pending:
        current = pending.pop()
        if current in included:
            continue
        included.add(current)
        pending.extend(dep["pkg"] for dep in nodes[current]["deps"])
    package = next(package for package in metadata["packages"] if package["id"] == root_id)
    # Cargo resolves CARGO_TARGET_DIR and repository configuration. A stale
    # repository-local executable must never replace the configured build.
    target_directory = Path(metadata["target_directory"])
    if not target_directory.is_absolute():
        raise RuntimeError("Cargo target directory must be absolute")
    binary = target_directory / "release" / ("yoke.exe" if sys.platform == "win32" else "yoke")
    if not binary.is_file() or binary.is_symlink():
        raise RuntimeError("Release executable unavailable or linked")
    if command(str(binary), "--version", timeout=10).strip() != f"yoke {package['version']}":
        raise RuntimeError("Release executable version differs from Cargo package version")
    output = args.output or repo / "dist-native" / f"yoke-{package['version']}-{host}.zip"
    if output.exists():
        raise RuntimeError("Archive exists; choose a new output path")
    portable_readme = f"""# Yoke native Rust {package['version']}

Run `yoke.exe --root C:/project index` on Windows, or `./yoke --root /project index` on Unix.
The default executable requires no Node, Python, upstream application, model or API key.
Use `--help` for commands and read [the native guide](native/README.md) for source scope,
optional semantic servers, editor integration, guarded changes and current limits.
This archive contains the executable, documentation and full license inventory;
it contains no captured workspace, account credential, private key or language server.
`manifest.json` records the exact executable checksum and locked build dependencies.
The independent advanced TypeScript runtime is distributed separately.
"""
    members = {binary.name: binary.read_bytes(), "LICENSE": (repo / "LICENSE").read_bytes(), "README.md": portable_readme.encode("utf-8"), "native/README.md": (repo / "native" / "README.md").read_bytes(), "SECURITY.md": (repo / "SECURITY.md").read_bytes(), "CHANGELOG.md": (repo / "CHANGELOG.md").read_bytes(), "THIRD_PARTY_NOTICES.md": (repo / "THIRD_PARTY_NOTICES.md").read_bytes(), "Cargo.lock": (repo / "Cargo.lock").read_bytes(), "UPSTREAMS.json": (repo / "UPSTREAMS.json").read_bytes()}
    for directory in [repo / "native/docs", repo / "docs"]:
        for path in sorted(directory.rglob("*.md")):
            if path.is_symlink():
                raise RuntimeError("Linked documentation input refused")
            if path.stat().st_size > 1_000_000:
                raise RuntimeError("Documentation byte budget exceeded")
            members[path.relative_to(repo).as_posix()] = path.read_bytes()
    for validation in sorted((repo / "docs/validation").glob("native-*")):
        # Archive digests and the packaged-host run are generated afterward in
        # a separate companion; including them would create circular evidence.
        if validation.name.startswith("native-distribution") or validation.suffix not in (".json", ".txt"):
            continue
        if validation.is_symlink() or not validation.is_file() or validation.stat().st_size > 1_000_000:
            raise RuntimeError("Unsupported validation evidence input")
        members["docs/validation/" + validation.name] = validation.read_bytes()
    inventory = []
    for dependency in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        if dependency["id"] not in included or dependency["id"] == root_id:
            continue
        crate_root = Path(dependency["manifest_path"]).parent
        licenses = [(path.name, path.read_bytes(), "locked-crate-package") for path in crate_root.iterdir() if path.is_file() and license_name(path.name)]
        if not licenses:
            if not args.fetch_missing_licenses:
                raise RuntimeError(f"License text missing for {dependency['name']}; review --fetch-missing-licenses")
            licenses = github_licenses(dependency, crate_root)
        license_records = []
        for name, data, provenance in licenses:
            if len(data) > 1_000_000:
                raise RuntimeError("License byte budget exceeded")
            member = f"licenses/{dependency['name']}-{dependency['version']}/{name}"
            members[member] = data
            license_records.append({"path": member, "source": provenance, "sha256": hashlib.sha256(data).hexdigest()})
        inventory.append({"name": dependency["name"], "version": dependency["version"], "spdx": dependency["license"], "repository": dependency["repository"], "licenseTexts": license_records})
    manifest = {"name": "yoke-code-intelligence", "version": package["version"], "target": host, "binarySha256": hashlib.sha256(members[binary.name]).hexdigest(), "dependencyScope": "Cargo resolved native build graph, including build-time crates; not a runtime module-load claim", "dependencies": inventory}
    members["manifest.json"] = (json.dumps(manifest, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in sorted(members.items()):
            info = zipfile.ZipInfo(name, date_time=(2026, 10, 14, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o755 if name == "yoke" else 0o644) << 16
            archive.writestr(info, data)
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None or json.loads(archive.read("manifest.json"))["binarySha256"] != hashlib.sha256(archive.read(binary.name)).hexdigest():
            raise RuntimeError("Archive validation failed")
    print(json.dumps({"archive": str(output.resolve()), "bytes": output.stat().st_size, "sha256": hashlib.sha256(output.read_bytes()).hexdigest(), "dependencies": len(inventory), "members": len(members)}))


if __name__ == "__main__":
    main()
