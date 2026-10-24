"""Read-only evidence parsing. Candidate runner code is never imported or executed."""
import ast
import hashlib
import json
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import stat

PHASES = ("format", "clippy", "native", "python", "typescript")
TOP_FILES = ("freeze.json", "source-freeze.json", "runner.py")
MAX_FILE = 256 * 1024 * 1024
MAX_FILES = 20_000
MAX_TOTAL = 1024 * 1024 * 1024
RUNNING = re.compile(r"^\s*Running .+ \((.+\.exe)\)\s*$")
LIST_END = re.compile(r"^(\d+) tests?, (\d+) benchmarks?$")
TEST_END = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;.*$")
TEST_ROW = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)(?:,.*)?$")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")


def valid_hash(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None


def relative_path(raw):
    require(isinstance(raw, str) and raw and "\\" not in raw, "Invalid portable evidence path")
    path = PurePosixPath(raw)
    require(not path.is_absolute() and path.as_posix() == raw, "Noncanonical relative path")
    for part in path.parts:
        require(part not in ("", ".", "..") and part == part.rstrip(" ."), "Traversal or path alias refused")
        require(not any(ord(c) < 32 or c in '<>:"|?*' for c in part), "Windows special path refused")
        require(re.fullmatch(r"CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9]", part.split(".")[0], re.I) is None, "Windows device name refused")
    return Path(*path.parts)


def unique_paths(names):
    names = list(names)
    for name in names:
        relative_path(name)
    require(len(names) == len(set(name.casefold() for name in names)), "Duplicate or case-aliased paths")


def plain(path, boundary, file=False, missing=False):
    path, boundary = Path(path), Path(boundary)
    require(path.is_absolute() and path.is_relative_to(boundary) and path != boundary, "Path escaped boundary")
    require(boundary.resolve(strict=True) == boundary, "Boundary resolves elsewhere")
    current = boundary
    for part in path.relative_to(boundary).parts:
        current = current / part
        try:
            info = current.lstat()
        except FileNotFoundError:
            require(missing, "Missing evidence: " + str(current))
            continue
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400, "Reparse evidence refused")
        require(current.resolve(strict=True) == current, "Resolved path changed")
        require(stat.S_ISREG(info.st_mode) if current == path and file else stat.S_ISDIR(info.st_mode), "Wrong evidence path type")
    return path


def read(path, boundary):
    path = plain(path, boundary, file=True)
    before = path.stat()
    require(before.st_size <= MAX_FILE, "Evidence file exceeds limit")
    with path.open("rb") as stream:
        data = stream.read(MAX_FILE + 1)
    after = plain(path, boundary, file=True).stat()
    require(len(data) == before.st_size <= MAX_FILE and
            (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) ==
            (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), "Evidence changed during read")
    return data


def load_json_bytes(data):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "Duplicate JSON field: " + key)
            result[key] = value
        return result
    try:
        return json.loads(data.decode("utf-8"), object_pairs_hook=pairs,
                          parse_constant=lambda _: (_ for _ in ()).throw(RuntimeError("Nonfinite JSON value")))
    except (UnicodeError, ValueError, TypeError) as error:
        raise RuntimeError("Invalid evidence JSON") from error


def load(path, boundary):
    return load_json_bytes(read(path, boundary))


def literal_contract(data):
    try:
        syntax = ast.parse(data.decode("utf-8"))
        values = {}
        for node in syntax.body:
            if isinstance(node, ast.Assign):
                for target in node.targets:
                    if isinstance(target, ast.Name) and target.id in ("BASE", "ACL_TESTS", "IGNORED_WORKER"):
                        require(target.id not in values, "Repeated runner contract literal")
                        values[target.id] = ast.literal_eval(node.value)
        require(set(values) == {"BASE", "ACL_TESTS", "IGNORED_WORKER"}, "Missing runner contract literals")
        require(isinstance(values["BASE"], list) and all(isinstance(x, str) for x in values["BASE"]), "Invalid BASE literal")
        acl = values["ACL_TESTS"]
        require(isinstance(acl, (tuple, list)) and acl and len(acl) == len(set(acl)) and all(isinstance(x, str) and x for x in acl), "Invalid ACL literal")
        require(isinstance(values["IGNORED_WORKER"], str) and values["IGNORED_WORKER"], "Invalid ignored-worker literal")
        values["ACL_TESTS"] = list(acl)
        return values
    except (SyntaxError, UnicodeError, ValueError, TypeError) as error:
        raise RuntimeError("Runner contract must contain literal data, not executable expressions") from error


def parse_python(data):
    text = "\n".join(data.decode("utf-8").splitlines())
    totals = re.findall(r"^Ran (\d+) tests? in .+$", text, re.M)
    names = re.findall(r"^(test_\S+ \([^\n]+\)) \.\.\. ok$", text, re.M)
    require(len(totals) == 1 and int(totals[0]) > 0 and len(names) == len(set(names)) == int(totals[0]), "Python names/counts incomplete or duplicated")
    require(re.findall(r"^OK$", text, re.M) == ["OK"], "Python result is not one clean successful suite")
    return names


def harness_groups(data, inventory):
    groups, active = [], None
    for line in data.decode("utf-8").splitlines():
        started = RUNNING.fullmatch(line)
        if started:
            require(active is None, "Missing prior Cargo harness summary")
            active = {"executable": started[1], "rows": {}}
            continue
        if inventory:
            row, end = re.fullmatch(r"(.+): (test|benchmark)", line), LIST_END.fullmatch(line)
            if row:
                require(active is not None and row[2] == "test", "Unsupported inventory row/benchmark")
                require(row[1] not in active["rows"], "Duplicate registered name")
                active["rows"][row[1]] = "registered"
            if end:
                require(active is not None and int(end[2]) == 0 and len(active["rows"]) == int(end[1]), "Inventory count differs")
                groups.append(active)
                active = None
        else:
            row, end = TEST_ROW.fullmatch(line), TEST_END.fullmatch(line)
            if row:
                require(active is not None and row[1] not in active["rows"], "Duplicate/outside-harness result")
                active["rows"][row[1]] = row[2]
            if end:
                require(active is not None, "Summary outside harness")
                active.update(passed=int(end[2]), failed=int(end[3]), ignored=int(end[4]), measured=int(end[5]), filtered=int(end[6]))
                require(active["measured"] == 0 and end[1] == "ok" and active["failed"] == 0, "Failed or measured native group")
                for key, status in (("passed", "ok"), ("failed", "FAILED"), ("ignored", "ignored")):
                    require(sum(x == status for x in active["rows"].values()) == active[key], "Native row/summary count differs")
                require(len(active["rows"]) == sum(active[key] for key in ("passed", "failed", "ignored")), "Incomplete native names")
                groups.append(active)
                active = None
    require(active is None, "Unterminated native harness")
    return groups


def group_map(groups, names):
    require(len(groups) == len(names), "Cargo harness count differs")
    mapped = {}
    for group in groups:
        leaf = group["executable"].replace("\\", "/").rsplit("/", 1)[-1]
        matches = [name for name in names if re.fullmatch(re.escape(name) + r"-[0-9a-f]+\.exe", leaf)]
        require(len(matches) == 1 and matches[0] not in mapped, "Unknown/duplicate executable target")
        mapped[matches[0]] = group
    require(set(mapped) == set(names), "Cargo target set differs")
    return mapped


def library_partition(registered, ignored, acl, ordinary, exact):
    require(set(ignored).issubset(registered) and set(acl).issubset(registered) and not set(acl).intersection(ignored), "ACL/ignored inventory differs")
    require({name for name in registered if any(skip in name for skip in acl)} == set(acl), "Ambiguous ACL substring selection")
    require(set(ordinary) == set(registered).difference(acl) and len(exact) == len(acl), "Library ordinary/exact selection differs")
    combined = dict(ordinary)
    for name, group in zip(acl, exact):
        require(group == {name: "ok"} and name not in combined, "ACL test did not pass exactly once")
        combined.update(group)
    require(set(combined) == set(registered), "Library coverage incomplete")
    require({name for name, status in combined.items() if status == "ignored"} == set(ignored), "Ignored tests relabeled or omitted")
    require(all(status in ("ok", "ignored") for status in combined.values()), "Failed library status")
    return combined


def check_manifest(manifest):
    require(isinstance(manifest, dict) and manifest, "Empty source manifest")
    unique_paths(manifest)
    require(all(valid_hash(value) for value in manifest.values()), "Malformed source digest")


def phase_receipt(bundle, phase, freeze, manifest):
    directory = plain(bundle / phase, bundle)
    receipt = load(directory / "receipt.json", bundle)
    require(receipt["phase"] == phase and receipt["phases"] == list(PHASES) and receipt["accepted"] is True, "Unaccepted or wrong phase")
    require(receipt["sourceChanges"] == [] and receipt["validationErrors"] == [], "Accepted phase contains changes/errors or invalid types")
    require(all(type(receipt[key]) is int and receipt[key] >= 0 for key in ("groups", "passed", "failed", "ignored")) and type(receipt["sourceFiles"]) is int and receipt["sourceFiles"] > 0, "Invalid phase count types")
    require(all(receipt[key] == freeze[key] for key in ("sourceManifestSha256", "runnerSha256", "sourceFiles")), "Phase freeze differs")
    require(load(directory / "source-before.json", bundle) == load(directory / "source-after.json", bundle) == manifest, "Phase source snapshots differ")
    results = receipt["results"]
    require(isinstance(results, list) and results, "Empty phase results")
    labels = [row["label"] for row in results]
    require(len(labels) == len(set(labels)) and all(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*", label) for label in labels), "Duplicate/unsafe result labels")
    required = {"source-before.json", "source-after.json", *(label + ".log" for label in labels)}
    if phase == "native":
        required.update(("target-inventory.json", "cleaned-before.json"))
        required.update(label[:-4] + ".json" for label in labels if label.endswith("-inventory-all"))
        required.update(label + "-cleaned.json" for label in labels if re.fullmatch(r"integration-[0-9]+", label))
    require(set(receipt["artifactSha256"]) == required, "Missing/unknown phase artifacts")
    for name, expected in receipt["artifactSha256"].items():
        require(len(relative_path(name).parts) == 1 and valid_hash(expected) and sha(read(directory / name, bundle)) == expected, "Phase artifact hash differs")
    for row in results:
        require(row["origin"] == "fresh_execution" and type(row["exitCode"]) is int and row["exitCode"] == 0 and row["failed"] == 0, "Failed/retained execution")
        require(all(row[key] == freeze[key] for key in ("sourceManifestSha256", "runnerSha256")), "Result source/runner differs")
        require(sha(read(directory / (row["label"] + ".log"), bundle)) == row["logSha256"], "Result log hash differs")
        require(isinstance(row["inventoryOnly"], bool) and all(type(row[key]) is int and row[key] >= 0 for key in ("groups", "passed", "failed", "ignored")), "Invalid result counts/role")
    require(all(receipt[key] == sum(row[key] for row in results) for key in ("groups", "passed", "failed", "ignored")), "Receipt total differs")
    require(PureWindowsPath(receipt["tempRoot"]).drive.upper() == "D:", "Ordinary fixtures must use D")
    require((phase == "native" and PureWindowsPath(receipt["aclTempRoot"]).drive.upper() == "C:") or (phase != "native" and receipt["aclTempRoot"] is None), "ACL fixture root differs")
    return receipt


def replay_static(bundle, phase, receipt):
    rows = {row["label"]: row for row in receipt["results"]}
    expected = {"format": {"format": ["cargo", "fmt", "--all", "--", "--check"]},
                "clippy": {"clippy": ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "-j", "2", "--", "-D", "warnings"]},
                "python": {"python": ["python", "-X", "utf8", "-m", "unittest", "discover", "-s", "scripts/tests", "-v"]},
                "typescript": {"typecheck": ["npm.cmd", "run", "typecheck"], "typescript": ["npm.cmd", "test"]}}[phase]
    require(set(rows) == set(expected), "Static result label set differs")
    for label, command in expected.items():
        row = rows[label]
        require(row["command"] == command and row["inventoryOnly"] is False and row["tempRoot"] == receipt["tempRoot"], "Static command/role/temp differs")
        counts = dict(groups=0, passed=0, failed=0, ignored=0)
        data = read(bundle / phase / (label + ".log"), bundle)
        if label == "python":
            names = parse_python(data)
            counts.update(groups=1, passed=len(names))
            require(row.get("testNames") == names, "Python named result list differs")
        if label == "typescript":
            values = re.findall(r"^(?:#|\u2139)\s+(tests|pass|fail|cancelled|skipped|todo)\s+(\d+)\s*$", data.decode("utf-8"), re.M)
            require(len(values) == len(dict(values)) == 6, "Missing/duplicate Node summary")
            parsed = {name: int(value) for name, value in values}
            require(parsed["pass"] > 0 and parsed["fail"] == parsed["cancelled"] == parsed["todo"] == 0 and parsed["tests"] == parsed["pass"] + parsed["skipped"], "Incomplete/failed Node suite")
            require(row.get("frameworkCounts") == parsed, "Node raw summary differs")
            counts.update(groups=1, passed=parsed["pass"], ignored=parsed["skipped"])
        require(all(row[key] == value for key, value in counts.items()), "Static raw/receipt counts differ")


def replay_native(bundle, receipt, contract, workspace, manifest):
    directory, base = bundle / "native", contract["BASE"]
    require(base == ["cargo", "test", "--workspace", "--locked", "-j", "2", "--no-fail-fast"], "Reviewed native command policy changed")
    results = {row["label"]: row for row in receipt["results"]}
    used = set()

    def result(label, command, inventory=False, acl=False):
        require(label in results and label not in used, "Missing/repeated native result")
        row = results[label]
        require(row["command"] == command and row["inventoryOnly"] is inventory, "Native command/role differs")
        require(row["tempRoot"] == receipt["aclTempRoot" if acl else "tempRoot"], "Native fixture root differs")
        if inventory:
            require(all(row[key] == 0 for key in ("groups", "passed", "failed", "ignored")), "Inventory contributes invented tests")
        used.add(label)
        return row, read(directory / (label + ".log"), bundle)

    _, metadata_raw = result("cargo-metadata", ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], True)
    metadata = load_json_bytes(metadata_raw)
    require(Path(metadata["target_directory"]) == workspace / "target", "Unexpected Cargo target directory")
    packages = [item for item in metadata["packages"] if item["id"] in metadata["workspace_members"]]
    require(len(packages) == 1 and packages[0]["name"] == "yoke-code-intelligence", "Workspace package policy changed")
    targets = load(directory / "target-inventory.json", bundle)
    require(targets == packages[0]["targets"], "Saved/raw Cargo targets differ")
    kinds = ("lib", "bin", "test", "example", "bench")
    require(all(target["kind"] in [[kind] for kind in kinds] for target in targets), "New target kind requires review")
    by_kind = {kind: sorted(target["name"] for target in targets if target["kind"] == [kind]) for kind in kinds}
    require(len(by_kind["lib"]) == 1 and by_kind["test"] and all(len(names) == len(set(names)) for names in by_kind.values()), "Missing/duplicate Cargo targets")
    for target in targets:
        source = Path(target["src_path"])
        require(source.is_relative_to(workspace) and source.relative_to(workspace).as_posix() in manifest, "Cargo target is outside frozen normal files")

    def inventory(prefix, selectors, names, empty=False):
        _, raw_all = result(prefix + "-all", base + selectors + ["--", "--list"], True)
        _, raw_ignored = result(prefix + "-ignored", base + selectors + ["--", "--ignored", "--list"], True)
        all_groups, ignored_groups = group_map(harness_groups(raw_all, True), names), group_map(harness_groups(raw_ignored, True), names)
        records = {}
        for name in names:
            registered, ignored = sorted(all_groups[name]["rows"]), sorted(ignored_groups[name]["rows"])
            require((empty or registered) and set(ignored).issubset(registered) and all_groups[name]["executable"] == ignored_groups[name]["executable"], "Invalid native inventory")
            records[name] = dict(registered=registered, ignored=ignored, executable=all_groups[name]["executable"])
        require(load(directory / (prefix + ".json"), bundle) == records, "Inventory JSON differs from raw names")
        return records

    def execute(label, selectors, records, selected=None, extra=(), acl=False):
        row, data = result(label, base + selectors + ["--", "--format", "pretty", "--test-threads=4", *extra], acl=acl)
        groups = group_map(harness_groups(data, False), list(records))
        checked = []
        for name, record in records.items():
            expected = set(record["registered"] if selected is None else selected[name])
            group = groups[name]
            require(expected.issubset(record["registered"]) and set(group["rows"]) == expected, "Missing/extra selected tests")
            require(group["executable"] == record["executable"] and group["filtered"] == len(record["registered"]) - len(expected), "Executed artifact/filter differs")
            require({key for key, status in group["rows"].items() if status == "ignored"} == expected.intersection(record["ignored"]), "Ignored status differs")
            checked.append(dict(target=name, **group))
        require(row["testGroups"] == checked and row["groups"] == len(groups) and all(row[key] == sum(group[key] for group in groups.values()) for key in ("passed", "failed", "ignored")), "Raw native result/count differs")
        if acl:
            require(re.search(rb"^\s*Compiling\s", data, re.M) is None, "C ACL fixture unexpectedly rebuilt")
        return groups

    library = inventory("library-inventory", ["--lib"], by_kind["lib"])
    lib_name, record = next(iter(library.items()))
    registered, ignored, acl = set(record["registered"]), set(record["ignored"]), contract["ACL_TESTS"]
    require(contract["IGNORED_WORKER"] in ignored, "Required worker is not ignored")
    ordinary = execute("library-d", ["--lib"], library, {lib_name: registered.difference(acl)}, [item for name in acl for item in ("--skip", name)])
    partitions = [dict(label="library-d", names=sorted(ordinary[lib_name]["rows"]))]
    exact = []
    for index, name in enumerate(acl):
        label = "library-c-acl-" + str(index)
        group = execute(label, ["--lib", name], library, {lib_name: {name}}, ["--exact"], True)
        exact.append(group[lib_name]["rows"])
        partitions.append(dict(label=label, names=sorted(group[lib_name]["rows"])))
    library_partition(registered, ignored, acl, ordinary[lib_name]["rows"], exact)
    coverage = dict(libraryRegistered=len(registered), libraryIgnored=len(ignored), libraryIgnoredNames=sorted(ignored), ignoredWorker=contract["IGNORED_WORKER"], libraryPartitions=partitions, libraryDisjointCoverageAccepted=True)
    for kind, selector in (("bin", "--bins"), ("example", "--examples"), ("bench", "--benches")):
        if by_kind[kind]:
            execute(kind + "-tests", [selector], inventory(kind + "-inventory", [selector], by_kind[kind], True))
        else:
            coverage[kind + "Targets"] = []
    docs, raw_docs = result("doctests", base + ["--doc", "--", "--test-threads=4"])
    doc_groups = harness_groups(re.sub(rb"^\s*Doc-tests (\S+)\r?$", rb" Running doctests (\1.exe)", raw_docs, flags=re.M), False)
    require(len(doc_groups) == 1 and doc_groups[0]["executable"] == lib_name + ".exe" and docs["testGroups"] == doc_groups and docs["groups"] == 1, "Doctest harness differs")
    require(all(docs[key] == sum(group[key] for group in doc_groups) for key in ("passed", "failed", "ignored")), "Doctest raw totals differ")
    selected = []
    labels = sorted((label for label in results if re.fullmatch(r"integration-[0-9]+", label)), key=lambda label: int(label.split("-")[1]))
    for index, label in enumerate(labels):
        require(label == "integration-" + str(index).zfill(2), "Integration batch labels are incomplete")
        command = results[label]["command"]
        require(command[:len(base)] == base and "--" in command[len(base):], "Invalid integration command")
        selectors = command[len(base):command.index("--", len(base))]
        require(selectors and len(selectors) % 2 == 0 and selectors[::2] == ["--test"] * (len(selectors) // 2), "Unexpected integration selectors")
        names = selectors[1::2]
        require(names == sorted(set(names)) and set(names).issubset(by_kind["test"]), "Invalid integration target batch")
        execute(label, selectors, inventory(label + "-inventory", selectors, names))
        selected.extend(names)
    require(selected == by_kind["test"] and len(selected) == len(set(selected)), "Integration targets omitted or duplicated")
    coverage.update(integrationTargets=len(selected), integrationTargetNames=selected, integrationCoverageAccepted=True, targetNamesByKind=by_kind)
    require(receipt["matrixCoverage"] == coverage and used == set(results), "Native coverage or result labels differ")
    return dict(cargoTargets=len(targets), integrationTargets=len(selected), libraryRegistered=len(registered), libraryIgnored=len(ignored), exactAclNames=acl, targetNamesByKind=by_kind, disjointLibraryCoverage=True)


def validate_bundle(bundle, expected_source, expected_runner, workspace):
    require(valid_hash(expected_source) and valid_hash(expected_runner), "Explicit reviewed source/runner hashes required")
    freeze, manifest = load(bundle / "freeze.json", bundle), load(bundle / "source-freeze.json", bundle)
    check_manifest(manifest)
    require(freeze["phases"] == list(PHASES) and type(freeze["sourceFiles"]) is int and freeze["sourceFiles"] == len(manifest), "Source count/phase inventory differs")
    require(sha(encoded(manifest)) == sha(read(bundle / "source-freeze.json", bundle)) == freeze["sourceManifestSha256"] == expected_source, "Source freeze differs")
    runner_raw = read(bundle / "runner.py", bundle)
    require(sha(runner_raw) == freeze["runnerSha256"] == expected_runner, "Reviewed runner differs")
    contract = literal_contract(runner_raw)
    summary = load(bundle / "summary.json", bundle)
    require(summary["accepted"] is True and all(summary[key] == value for key, value in freeze.items()) and set(summary["phaseReceipts"]) == set(PHASES), "Missing/mismatched finalized five-phase summary")
    require(summary["freezeArtifactSha256"] == {name: sha(read(bundle / name, bundle)) for name in TOP_FILES}, "Summary freeze pins differ")
    receipts = {}
    for phase in PHASES:
        receipt = phase_receipt(bundle, phase, freeze, manifest)
        require(summary["phaseReceipts"][phase] == dict(receiptSha256=sha(read(bundle / phase / "receipt.json", bundle)), **{key: receipt[key] for key in ("groups", "passed", "failed", "ignored")}), "Summary receipt pins/counts differ")
        if phase != "native":
            replay_static(bundle, phase, receipt)
        receipts[phase] = receipt
    matrix = replay_native(bundle, receipts["native"], contract, workspace, manifest)
    return dict(freeze=freeze, manifest=manifest, receipts=receipts, nativeMatrix=matrix)
