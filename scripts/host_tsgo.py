"""Hosted Windows real TSGo qualification, using the same-run fresh CLI producer.

Imports, archive/image reads and vendor/native execution are CI-only. Local tests
extract only pure admission functions; they never import this controller.
"""
import argparse
import base64
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath
import re
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "validation/ci-tsgo-acceptance"
WORKERS = ROOT / "validation/ci-tsgo-workers"
VERSION = "7.0.0-dev.20260707.2"
URL = "https://registry.npmjs.org/@typescript/native-preview-win32-x64/-/native-preview-win32-x64-7.0.0-dev.20260707.2.tgz"
INTEGRITY = "DL4u27stv0fo71sVhOzHSwE+YMZsbBijVI+kg5dLDLilSH79WFTJ8RSQ46vJrCMt+Gjlv/JOZP1PuLJDfioYeQ=="
FILES = 113
EXPANDED = 28_366_072
ARCHIVE_MAX = 32_000_000
TAR_MAX = 30_000_000


def sibling():
    spec = importlib.util.spec_from_file_location("host_vscode", ROOT / "scripts/host_vscode.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


V = sibling()
JOB = V.JOB


def require(condition, message):
    if not condition:
        raise ValueError(message)


def admit_members(records, expected_count, expected_bytes):
    require(type(expected_count) is int and 0 < expected_count <= 256
            and type(expected_bytes) is int and 0 < expected_bytes <= 30_000_000
            and isinstance(records, list) and 0 < len(records) <= 256, "Package inventory capacity")
    names, files, total = {}, {}, 0
    for row in records:
        require(isinstance(row, dict) and set(row) == {"name", "bytes", "kind"}, "Package member fields differ")
        name, size, kind = row["name"], row["bytes"], row["kind"]
        require(isinstance(name, str) and 0 < len(name) <= 4096 and "\\" not in name
                and kind in ("file", "directory") and type(size) is int and 0 <= size <= 30_000_000,
                "Special or oversized package member")
        parts = name.rstrip("/").split("/") if kind == "directory" else name.split("/")
        require(parts[0] == "package" and (len(parts) > 1 or kind == "directory"), "Package namespace differs")
        for part in parts:
            require(part not in ("", ".", "..") and part[-1] not in " ."
                    and len(part.encode("utf-16-le")) <= 510
                    and not any(ord(c) < 32 or ord(c) == 127 or c in ':*?"<>|' for c in part), "Unsafe package path")
            device = part.split(".", 1)[0].casefold()
            require(device not in {"con", "prn", "aux", "nul", "conin$", "conout$"}
                    and re.fullmatch(r"(?:com|lpt)[1-9\u00b9\u00b2\u00b3]", device) is None, "Device package member")
        key = "/".join(parts).casefold()
        require(key not in names, "Duplicate or aliased package member")
        names[key] = kind
        if kind == "directory":
            require(size == 0, "Directory payload refused")
        else:
            files[name] = size
            total += size
            require(total <= expected_bytes, "Package expanded byte capacity")
    for key in names:
        parts = key.split("/")
        require(all(names.get("/".join(parts[:i]), "directory") == "directory" for i in range(1, len(parts))), "Package parent is a file")
    require(len(files) == expected_count and total == expected_bytes and "package/package.json" in files
            and [name for name in files if name.casefold().endswith(".exe")] == ["package/lib/tsgo.exe"], "Pinned package inventory differs")
    return files


def admit_package(value):
    require(isinstance(value, dict) and value.get("name") == "@typescript/native-preview-win32-x64"
            and value.get("version") == "7.0.0-dev.20260707.2"
            and value.get("os") == ["win32"] and value.get("cpu") == ["x64"], "Vendor package identity differs")


def fixture_sources():
    return {
        "entry.ts": ('import { greet } from "./src/Gr\u00fc\u00dfe e\u0301";\r\n'
                     'export const message = /* \U0001f4a1 e\u0301 */ greet("\u4e16\u754c");\r\n').encode("utf8"),
        "src/Gr\u00fc\u00dfe e\u0301.ts": ('// owned \U0001f4a1 e\u0301\r\n'
                     'export function greet(name: string): string { return "Hallo " + name; }\r\n').encode("utf8"),
        "tsconfig.json": (json.dumps({"compilerOptions": {"strict": True, "target": "ES2020", "module": "ESNext",
            "moduleResolution": "bundler", "noEmit": True, "types": []}, "typeAcquisition": {"enable": False},
            "include": ["entry.ts", "src/*.ts"]}, ensure_ascii=True) + "\r\n").encode(),
    }


def token_spans(sources):
    return {name: [(m.start(), m.end()) for m in re.finditer(rb"\bgreet\b", raw)]
            for name, raw in sources.items() if name.endswith(".ts")}


def utf16_range(raw, start, end):
    def position(offset):
        prefix = raw[:offset].decode("utf8")
        return {"line": prefix.count("\n"), "character": len(prefix.rsplit("\n", 1)[-1].encode("utf-16-le")) // 2}
    return {"start": position(start), "end": position(end)}


def admit_reply(value, sources, executable, vendor_hash):
    require(isinstance(value, dict) and set(value) == {"results", "sourceWritten", "atomicBatch"}
            and value["sourceWritten"] is False and value["atomicBatch"] is False, "Session effect scope differs")
    rows = value["results"]
    require(isinstance(rows, list) and len(rows) == 3, "Three vendor operations required")
    spans = token_spans(sources)
    entry, helper = "entry.ts", "src/Gr\u00fc\u00dfe e\u0301.ts"
    snapshot, checked = None, []
    for index, row in enumerate(rows):
        require(isinstance(row, dict) and row.get("status") == "ok" and isinstance(row.get("data"), dict), "Native query failed")
        data = row["data"]
        require(isinstance(data.get("snapshotId"), str) and 0 < len(data["snapshotId"]) <= 256, "Snapshot binding absent")
        snapshot = snapshot or data["snapshotId"]
        require(data["snapshotId"] == snapshot and data.get("producer") == executable
                and data.get("complete") is False and data.get("sourceExecutionAuthorized") is True
                and data.get("selectedBackendId") == "typescript_tsgo", "Provider source/scope differs")
        selection = data.get("backendSelection")
        require(isinstance(selection, dict) and selection == {"policy": "first_supporting_file_then_first_fallback",
            "matchedDeclaredExtension": True, "fallback": False, "runtimeCertification": False}
            and selection["matchedDeclaredExtension"] is True and selection["fallback"] is False
            and selection["runtimeCertification"] is False, "Backend selection scope differs")
        binding = data.get("serverProfileBinding")
        require(isinstance(binding, dict) and binding.get("kind") == "selected_command_bytes"
                and binding.get("contentHash") == vendor_hash, "Selected vendor bytes differ")
        if index == 0:
            response = data.get("response")
            require(isinstance(response, dict) and response.get("range") == utf16_range(sources[entry], *spans[entry][-1]), "Hover range differs")
            text = json.dumps(response.get("contents"), ensure_ascii=True)
            require(0 < len(text) <= 32768 and "greet" in text and "string" in text, "Typed real hover absent")
            continue
        locations = data.get("locations")
        require(isinstance(locations, list) and 0 < len(locations) <= 8, "Nonempty bounded locations required")
        seen = set()
        for location in locations:
            require(isinstance(location, dict), "Location object required")
            name, start, end = location.get("path"), location.get("startByte"), location.get("endByte")
            require(isinstance(name, str) and name in spans and type(start) is int and type(end) is int
                    and (start, end) in spans[name] and (name, start, end) not in seen, "Foreign/invalid/duplicate byte anchor")
            require(location.get("contentHash") == hashlib.sha256(sources[name]).hexdigest()
                    and sources[name][start:end] == b"greet" and location.get("text") == "greet"
                    and location.get("producer") == executable and location.get("resolution") == "exact_binding"
                    and location.get("trust") == "source-data", "Location raw source binding differs")
            seen.add((name, start, end))
        expected = {(helper, *spans[helper][0])} if index == 1 else {(name, *span) for name in spans for span in spans[name]}
        require(seen == expected, "Fixture definition/reference set differs")
        checked.append(len(seen))
    return {"snapshotId": snapshot, "hoverRangeChecked": True, "definitionLocations": checked[0], "referenceLocations": checked[1]}


def admit_state(value, revision, root, temp):
    require(isinstance(value, dict) and set(value) == {"schemaVersion", "sourceRevision", "toolRoot", "workerRoots"}
            and type(value["schemaVersion"]) is int and value["schemaVersion"] == 1
            and value["sourceRevision"] == revision, "Vendor state source/schema differs")
    tool = PureWindowsPath(value["toolRoot"])
    require(tool.parent == temp and not tool.is_relative_to(root) and re.fullmatch(r"yts-[0-9a-f]{16}", tool.name) is not None, "Private vendor root required")
    workers = value["workerRoots"]
    require(isinstance(workers, dict) and set(workers) == {"tool", "query"}, "Vendor worker roles differ")
    for phase, name in workers.items():
        path = PureWindowsPath(name)
        require(path.parent == temp and not path.is_relative_to(root)
                and re.fullmatch("ytw-" + phase + r"-[0-9a-f]{16}", path.name) is not None, "Private worker root required")
    return value


def admit_result(value, phase, revision):
    require(isinstance(value, dict) and set(value) == {"schemaVersion", "sourceRevision", "phase", "complete", "workerAdmission",
        "nativeSHA256", "vendorVersion", "sourceInputsUnchanged", "builtInputsUnchanged", "observation"}, "Vendor result fields differ")
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1 and value["sourceRevision"] == revision
            and value["phase"] == phase and value["complete"] is True and value["vendorVersion"] == "7.0.0-dev.20260707.2"
            and value["sourceInputsUnchanged"] is True and value["builtInputsUnchanged"] is True
            and isinstance(value["nativeSHA256"], str) and re.fullmatch(r"[0-9a-f]{64}", value["nativeSHA256"]) is not None
            and isinstance(value["observation"], dict), "Vendor result source/scope incomplete")
    admission = value["workerAdmission"]
    require(isinstance(admission, dict) and set(admission) == {"specificJobMembership", "queryHandleClosedBeforePayloadRead"}
            and all(flag is True for flag in admission.values()), "Owned worker admission incomplete")


def load_state(revision):
    return admit_state(V.read_json(WORKERS / "state.json"), revision, PureWindowsPath(str(ROOT)), PureWindowsPath(os.environ["RUNNER_TEMP"]))


def initialize(revision):
    JOB.ordinary(ROOT / "validation", True)
    require(not OUTPUT.exists() and not WORKERS.exists(), "Fresh vendor namespaces required")
    V.admit_lifecycle(V.read_json(ROOT / "validation/ci-vscode-lifecycle-proof/result.json"), revision)
    producer = V.read_json(V.OUTPUT / "producer.json")
    require(producer["sourceRevision"] == revision and producer["complete"] is True and producer["freshTargetAndDist"] is True, "Same-run fresh CLI required")
    temp = Path(os.environ["RUNNER_TEMP"])
    JOB.ordinary(temp, True)
    value = {"schemaVersion": 1, "sourceRevision": revision, "toolRoot": str(temp / ("yts-" + os.urandom(8).hex())),
             "workerRoots": {phase: str(temp / ("ytw-" + phase + "-" + os.urandom(8).hex())) for phase in ("tool", "query")}}
    admit_state(value, revision, PureWindowsPath(str(ROOT)), PureWindowsPath(str(temp)))
    for path in [Path(value["toolRoot"]), *(Path(p) for p in value["workerRoots"].values())]:
        require(not path.exists(), "Private namespace occupied")
    OUTPUT.mkdir()
    WORKERS.mkdir()
    V.write_json(WORKERS / "state.json", value)


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("Vendor redirect refused")


def procure(tool):
    require(not tool.exists(), "Fresh vendor extraction required")
    require(shutil.disk_usage(tool.parent).free >= JOB.MIN_FREE + ARCHIVE_MAX + TAR_MAX + EXPANDED, "Vendor volume reserve unavailable")
    tool.mkdir()
    archive_path, tar_path = tool / "vendor.tgz", tool / "vendor.tar"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    deadline, total, digest = time.monotonic() + 120, 0, hashlib.sha512()
    request = urllib.request.Request(URL, headers={"Accept-Encoding": "identity"})
    with opener.open(request, timeout=20) as response, archive_path.open("xb") as output:
        require(response.status == 200 and response.geturl() == URL
                and response.headers.get("Content-Encoding", "identity") == "identity", "Official vendor response differs")
        while block := response.read(1_000_000):
            total += len(block)
            require(time.monotonic() < deadline and total <= ARCHIVE_MAX
                    and shutil.disk_usage(tool).free >= JOB.MIN_FREE + len(block), "Vendor download budget")
            digest.update(block)
            output.write(block)
    require(total > 0 and digest.digest() == base64.b64decode(INTEGRITY, validate=True), "Pinned vendor integrity differs")
    expanded = 0
    with gzip.open(archive_path, "rb") as source, tar_path.open("xb") as output:
        while block := source.read(1_000_000):
            expanded += len(block)
            require(expanded <= TAR_MAX and time.monotonic() < deadline
                    and shutil.disk_usage(tool).free >= JOB.MIN_FREE + len(block), "Vendor decompression budget")
            output.write(block)
    records, members = [], []
    with tarfile.open(tar_path, "r:") as archive:
        for info in archive:
            require(len(records) < 256 and info.sparse is None
                    and info.type in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE), "Vendor archive entry/sparse capacity")
            records.append({"name": info.name, "bytes": info.size,
                            "kind": "file" if info.isfile() else "directory" if info.isdir() else "special"})
            members.append(info)
        pins = admit_members(records, FILES, EXPANDED)
        destination = tool / "unpacked"
        destination.mkdir()
        hashes = {}
        for info in members:
            if info.name not in pins:
                continue
            target = destination.joinpath(*info.name.split("/"))
            target.parent.mkdir(parents=True, exist_ok=True)
            JOB.ordinary(target.parent, True)
            digest, written = hashlib.sha256(), 0
            with archive.extractfile(info) as source, target.open("xb") as output:
                while block := source.read(1_000_000):
                    written += len(block)
                    require(written <= pins[info.name] and time.monotonic() < deadline
                            and shutil.disk_usage(tool).free >= JOB.MIN_FREE + len(block), "Vendor extraction budget")
                    digest.update(block)
                    output.write(block)
            require(written == pins[info.name], "Vendor member length differs")
            hashes[info.name] = digest.hexdigest()
    admit_package(V.read_json(destination / "package/package.json"))
    V.write_json(tool / "inputs.json", hashes)
    V.check_tree(destination, hashes, EXPANDED, EXPANDED)
    return {"packageFiles": len(hashes), "packageBytes": sum(pins.values()), "archiveBytes": total,
            "archiveSHA512": base64.b64encode(hashlib.sha512(V.read_bytes(archive_path, ARCHIVE_MAX)).digest()).decode(),
            "inputMapSHA256": V.hash_file(tool / "inputs.json", 1_000_000)}


def child_environment():
    # Only the owned Job's redirected homes/temp and basic OS lookup survive.
    keys = ("PATH", "SystemRoot", "SystemDrive", "WINDIR", "USERPROFILE", "HOME", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP")
    value = {key: os.environ[key] for key in keys if key in os.environ}
    value.update(DO_NOT_TRACK="1", NO_COLOR="1")
    return value


def command(arguments, cwd, logs, name, timeout):
    startup = subprocess.STARTUPINFO()
    startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    with open(os.devnull, "rb") as stdin, (logs / (name + ".stdout")).open("xb") as stdout, (logs / (name + ".stderr")).open("xb") as stderr:
        result = subprocess.run([str(a) for a in arguments], cwd=cwd, env=child_environment(), stdin=stdin,
            stdout=stdout, stderr=stderr, startupinfo=startup, creationflags=subprocess.CREATE_NO_WINDOW, timeout=timeout)
    require(result.returncode == 0, "Selected vendor/native command failed")
    require((logs / (name + ".stderr")).stat().st_size <= 1_000_000, "Command stderr capacity")
    return V.read_bytes(logs / (name + ".stdout"), 1_000_000)


def query(tool, fixture, native, vendor_hash):
    sources = fixture_sources()
    workspace = fixture / "workspace \u03c0"
    workspace.mkdir()
    for name, raw in sources.items():
        path = workspace / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("xb") as stream:
            stream.write(raw)
    vendor = tool / "unpacked/package/lib/tsgo.exe"
    V.write_json(fixture / "servers.json", [{"backendId": "typescript_tsgo", "command": str(vendor),
        "args": ["--lsp", "--stdio"], "configuration": {"allowSourceExecution": True}}])
    V.write_json(fixture / "host.json", {"languageServers": {"global": ["typescript_tsgo"]}})
    offset = token_spans(sources)["entry.ts"][-1][0]
    V.write_json(fixture / "requests.json", [{"operation": "queryProject", "args": {
        "project": "default", "tool": "ci_find", "arguments": {
            "languageServer": {"path": "entry.ts", "offset": offset, "operation": operation}, "maxBytes": 64000}}}
        for operation in ("hover", "definition", "references")])
    raw = command([native, "--root", workspace, "--timeout", "100000", "--max-bytes", "64000",
        "--language-server-config", fixture / "servers.json", "--trust-servers", "session", "--host-profile", fixture / "host.json",
        "--requests", fixture / "requests.json"], workspace, fixture, "session", 120)
    value = V.decode(raw)
    checked = admit_reply(value, sources, str(vendor), vendor_hash)
    require(all(V.read_bytes(workspace / name, 65536) == body for name, body in sources.items()), "Fixture source changed")
    V.write_json(OUTPUT / "protocol.json", value)
    return {**checked, "savedFixtureSourceUnchanged": True, "vendorSHA256": vendor_hash,
            "operations": ["hover", "definition", "references"], "sourceSHA256": {n: hashlib.sha256(b).hexdigest() for n, b in sources.items()},
            "protocolSHA256": hashlib.sha256(raw).hexdigest(), "sourceExecutionGrant": "explicit_host_owned_fixture_only"}


def worker(phase):
    revision, _ = V.identity()
    admission = V.confirm_job()  # Close the specific-Job query handle before image/archive reads.
    state = load_state(revision)
    source, built, producer = (V.read_json(V.OUTPUT / name) for name in ("source-inputs.json", "built-inputs.json", "producer.json"))
    require(source["sourceRevision"] == built["sourceRevision"] == producer["sourceRevision"] == revision
            and producer["complete"] is True and producer["freshTargetAndDist"] is True and producer["sourceInputsUnchanged"] is True
            and producer["command"] == V.cargo_command() and V.source_inputs() == source["sha256"]
            and V.built_inputs() == built["sha256"], "Fresh source/producer inputs changed")
    role = V.producer_role(V.read_bytes(V.OUTPUT / "producer.stdout", 16_000_000), PureWindowsPath(str(ROOT)), producer["packageId"])
    native = Path(role["path"])
    require(role["path"] == producer["nativeRole"]["path"]
            and V.metadata(native, V.NATIVE_MAX) == producer["nativeRole"]["metadata"], "Fresh CLI role metadata differs")
    native_hash = V.hash_file(native, V.NATIVE_MAX)
    require(V.metadata(native, V.NATIVE_MAX) == producer["nativeRole"]["metadata"], "Fresh CLI changed while hashing")
    tool, fixture = Path(state["toolRoot"]), Path(state["workerRoots"][phase])
    vendor = tool / "unpacked/package/lib/tsgo.exe"
    if phase == "tool":
        observation = procure(tool)
        vendor_hash = V.hash_file(vendor, EXPANDED)
        observation.update(vendorSHA256=vendor_hash, vendorMetadata=V.metadata(vendor, EXPANDED))
        version = command([vendor, "--version"], fixture, fixture, "version", 10).decode("utf8").strip()
        require(version == "Version " + VERSION, "Actual vendor version differs")
        observation["actualVersion"] = version
    else:
        prior = V.read_json(OUTPUT / "tool.json")
        admit_result(prior, "tool", revision)
        require(prior["nativeSHA256"] == native_hash, "Producer changed between vendor stages")
        observation = prior["observation"]
        vendor_hash = observation["vendorSHA256"]
        require(V.hash_file(tool / "inputs.json", 1_000_000) == observation["inputMapSHA256"]
                and V.metadata(vendor, EXPANDED) == observation["vendorMetadata"], "Vendor inputs changed")
        V.check_tree(tool / "unpacked", V.read_json(tool / "inputs.json"), EXPANDED, EXPANDED)
        require(V.hash_file(vendor, EXPANDED) == vendor_hash, "Selected vendor bytes changed")
        observation = query(tool, fixture, native, vendor_hash)
    V.check_tree(tool / "unpacked", V.read_json(tool / "inputs.json"), EXPANDED, EXPANDED)
    require(V.source_inputs() == source["sha256"] and V.built_inputs() == built["sha256"]
            and V.metadata(native, V.NATIVE_MAX) == producer["nativeRole"]["metadata"]
            and V.hash_file(native, V.NATIVE_MAX) == native_hash and V.hash_file(vendor, EXPANDED) == vendor_hash,
            "Vendor/native/source inputs changed during stage")
    V.write_json(OUTPUT / (phase + ".json"), {"schemaVersion": 1, "sourceRevision": revision, "phase": phase, "complete": True,
        "workerAdmission": admission, "nativeSHA256": native_hash, "vendorVersion": VERSION,
        "sourceInputsUnchanged": True, "builtInputsUnchanged": True, "observation": observation})
    return 0


def stage(phase):
    revision, _ = V.identity()
    if phase == "tool":
        initialize(revision)
    state = load_state(revision)
    if phase == "query":
        JOB.admit_receipt(V.read_json(OUTPUT / "tool-job.json"))
        admit_result(V.read_json(OUTPUT / "tool.json"), "tool", revision)
    fixture = Path(state["workerRoots"][phase])
    require(not fixture.exists() and not (OUTPUT / (phase + ".json")).exists(), "Fresh vendor stage required")
    fixture.mkdir()
    try:
        row = JOB.run_owned(Path(sys.executable), ["-I", str(Path(__file__).resolve()), "--worker", phase], fixture, 180)
    finally:
        for name in ("job.json", "stdout.log", "stderr.log", "version.stdout", "version.stderr", "session.stdout", "session.stderr"):
            path = fixture / name
            if path.exists():
                with (OUTPUT / (phase + "-" + name)).open("xb") as stream:
                    stream.write(V.read_bytes(path, 4_000_000))
    JOB.admit_receipt(row)
    admit_result(V.read_json(OUTPUT / (phase + ".json")), phase, revision)
    return 0


def finish():
    revision, _ = V.identity()
    rows = []
    for phase in ("tool", "query"):
        JOB.admit_receipt(V.read_json(OUTPUT / (phase + "-job.json")))
        value = V.read_json(OUTPUT / (phase + ".json"))
        admit_result(value, phase, revision)
        rows.append(value)
    require(rows[0]["nativeSHA256"] == rows[1]["nativeSHA256"], "Vendor stage CLI identities differ")
    query_result = rows[1]["observation"]
    require(query_result["savedFixtureSourceUnchanged"] is True and query_result["hoverRangeChecked"] is True
            and query_result["definitionLocations"] == 1 and query_result["referenceLocations"] == 3
            and query_result["vendorSHA256"] == rows[0]["observation"]["vendorSHA256"], "Vendor operation evidence incomplete")
    state = load_state(revision)
    vendor = str(Path(state["toolRoot"]) / "unpacked/package/lib/tsgo.exe")
    raw = V.read_bytes(OUTPUT / "query-session.stdout", 1_000_000)
    require(hashlib.sha256(raw).hexdigest() == query_result["protocolSHA256"]
            and V.decode(raw) == V.read_json(OUTPUT / "protocol.json"), "Protocol evidence changed")
    admit_reply(V.decode(raw), fixture_sources(), vendor, query_result["vendorSHA256"])
    require(V.source_inputs() == V.read_json(V.OUTPUT / "source-inputs.json")["sha256"]
            and V.built_inputs() == V.read_json(V.OUTPUT / "built-inputs.json")["sha256"], "Final source/built inputs changed")
    V.write_json(OUTPUT / "result.json", {"schemaVersion": 1, "sourceRevision": revision, "complete": True,
        "product": "Code Intelligence", "nativeVersion": "0.5.0", "backendId": "typescript_tsgo", "vendorVersion": VERSION,
        "scope": "Pinned real Windows TSGo hover/definition/references on owned Unicode CRLF source through public native Session CLI",
        "allOwnedTreesExitedBeforeCleanup": True, "originalSourceBytesUnchanged": True,
        "passiveRuntimeCertification": False, "allBackendPlatformMainOrReleaseAccepted": False})
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("stage", "finish"), nargs="?")
    parser.add_argument("--phase", choices=("tool", "query"))
    parser.add_argument("--worker", choices=("tool", "query"), help=argparse.SUPPRESS)
    args = parser.parse_args()
    require((args.worker is not None) != (args.operation is not None), "Select one vendor operation")
    if args.worker:
        return worker(args.worker)
    require((args.operation == "stage") == (args.phase is not None), "Select one fixed vendor phase")
    return stage(args.phase) if args.operation == "stage" else finish()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print("Hosted TSGo qualification refused: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
