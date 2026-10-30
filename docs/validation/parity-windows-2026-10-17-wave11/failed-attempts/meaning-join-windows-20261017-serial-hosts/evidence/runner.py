"""Prepare fresh Windows acceptance evidence on one shared five-phase freeze.

Run each phase with the same --tag and a fresh E: --temp. Native also requires
a fresh C: --acl-temp for exactly three ACL fixtures and the ignored_capture
integration target, which requires genuine 8.3 aliases. The optional finalize
command verifies all five receipts; it does not execute another test phase.
This runner never imports or retains results from an earlier acceptance run.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
EVIDENCE = Path(__file__).resolve().parent
RUNNER = Path(__file__).resolve()
BUILD = Path("E:/Yoke-Build-Cache/meaning-aliases-20261016")
PHASES = ("format", "clippy", "native", "python", "typescript")
ACL_TESTS = (
    "host_install::io::tests::explicit_low_integrity_label_is_preserved_or_refused_before_human_bytes",
    "host_install::io::tests::prepared_output_preserves_owner_and_nondefault_group_before_human_bytes",
    "host_install::io::tests::staged_registered_reopen_checks_original_security_before_any_mutation",
)
IGNORED_WORKER = "host_install::output_recovery_tests::output_recovery_worker"
BASE = ["cargo", "test", "--workspace", "--locked", "-j", "1", "--no-fail-fast"]
RUNNING = re.compile(r"^\s*Running .+ \((.+\.exe)\)\s*$")
LIST_END = re.compile(r"^(\d+) tests?, (\d+) benchmarks?$")
TEST_END = re.compile(
    r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out;.*$"
)
TEST_ROW = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)(?:,.*)?$")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def encoded(value):
    return (json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n").encode("utf-8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    with path.open("xb") as stream:
        stream.write(encoded(value))


def load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def source_manifest():
    # Exactly Git's tracked plus normally unignored inventory, including new
    # product files. Only normal Git exclusions apply; no custom file filter.
    raw = subprocess.check_output(
        ["rtk", "proxy", "git", "ls-files", "-c", "-o", "--exclude-standard", "-z"], cwd=ROOT
    )
    return {
        name: digest((ROOT / name).read_bytes())
        for name in sorted(set(raw.decode("utf-8").split("\0")))
        if name and (ROOT / name).is_file()
    }


def checked_tag(tag):
    require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,100}", tag) is not None, "Unsafe evidence tag")
    return EVIDENCE / tag


def new_temp(raw, drive):
    require(raw is not None, "Missing explicit fixture temp root")
    supplied = Path(raw)
    require(supplied.is_absolute(), "Fixture temp root must be absolute")
    resolved = supplied.resolve()
    require(resolved.drive.upper() == drive + ":", "Unexpected fixture temp drive: " + str(resolved))
    require(len(resolved.parts) >= 2 and not resolved.exists(), "Fixture root must be a fresh non-drive path")
    require(not resolved.is_relative_to(ROOT), "Fixtures must be outside the source checkout")
    resolved.mkdir(parents=True, exist_ok=False)
    return resolved


def open_freeze(bundle):
    manifest = source_manifest()
    require(manifest, "Empty source inventory")
    current = {"sourceManifestSha256": digest(encoded(manifest)), "sourceFiles": len(manifest),
               "runnerSha256": digest(RUNNER.read_bytes()), "phases": list(PHASES)}
    if not bundle.exists():
        bundle.mkdir()
        write_json(bundle / "source-freeze.json", manifest)
        (bundle / "runner.py").write_bytes(RUNNER.read_bytes())
        write_json(bundle / "freeze.json", current)
    else:
        require(bundle.is_dir() and not bundle.is_symlink(), "Invalid existing freeze bundle")
        require(load_json(bundle / "freeze.json") == current, "Source or runner differs from shared freeze")
        require(load_json(bundle / "source-freeze.json") == manifest, "Shared manifest differs")
        require(digest((bundle / "runner.py").read_bytes()) == current["runnerSha256"], "Frozen runner copy differs")
    return manifest, current


def harness_groups(data, inventory):
    """Read Cargo's fresh per-executable list/results without merging names."""
    groups, active = [], None
    for line in data.decode("utf-8", errors="strict").splitlines():
        started = RUNNING.fullmatch(line)
        if started:
            require(active is None, "Missing harness summary before next executable")
            active = {"executable": started[1], "rows": {}}
            continue
        if inventory:
            row = re.fullmatch(r"(.+): (test|benchmark)", line)
            end = LIST_END.fullmatch(line)
            if row:
                require(active is not None, "Inventory row outside a Cargo target")
                require(row[2] == "test", "Unexpected benchmark entry requires an explicit runner contract")
                require(row[1] not in active["rows"], "Duplicate registered test name")
                active["rows"][row[1]] = "registered"
            if end:
                require(active is not None and int(end[2]) == 0, "Unexpected inventory summary")
                require(len(active["rows"]) == int(end[1]), "Inventory name/count mismatch")
                groups.append(active)
                active = None
        else:
            row = TEST_ROW.fullmatch(line)
            end = TEST_END.fullmatch(line)
            if row:
                require(active is not None, "Test result outside a Cargo target")
                require(row[1] not in active["rows"], "Duplicate executed test name")
                active["rows"][row[1]] = row[2]
            if end:
                require(active is not None, "Result summary outside a Cargo target")
                active.update(passed=int(end[2]), failed=int(end[3]), ignored=int(end[4]),
                              measured=int(end[5]), filtered=int(end[6]))
                require(active["measured"] == 0, "Unexpected measured benchmark results")
                for key, status in (("passed", "ok"), ("failed", "FAILED"), ("ignored", "ignored")):
                    require(sum(value == status for value in active["rows"].values()) == active[key],
                            "Per-test lines disagree with " + key + " summary")
                require(len(active["rows"]) == sum(active[key] for key in ("passed", "failed", "ignored")),
                        "Incomplete per-test result lines")
                groups.append(active)
                active = None
    require(active is None, "Unterminated Cargo harness output")
    return groups


def executable_target(executable, names):
    leaf = executable.replace("\\", "/").rsplit("/", 1)[-1]
    matched = [name for name in names if re.fullmatch(re.escape(name) + r"-[0-9a-f]+\.exe", leaf)]
    require(len(matched) == 1, "Unexpected or ambiguous executable: " + executable)
    return matched[0]


def group_map(groups, names):
    require(len(groups) == len(names), "Unexpected Cargo harness group count")
    mapped = {}
    for group in groups:
        name = executable_target(group["executable"], names)
        require(name not in mapped, "Duplicate executable target")
        mapped[name] = group
    require(set(mapped) == set(names), "Cargo target name drift")
    return mapped


def checked_cleanup(deps, names):
    """Remove only matching disposable leaves after synchronous Cargo returns."""
    target = BUILD
    require(target.resolve() == target and deps.resolve() == deps, "Reparse target path refused")
    require(deps == target / "debug" / "deps", "Unexpected cleanup directory")
    for directory in (ROOT, target.parent, target, target / "debug", deps):
        info = directory.lstat()
        require(stat.S_ISDIR(info.st_mode) and not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                "Cleanup directory is not a plain directory")
    removed = []
    pattern = re.compile(r"(?:" + "|".join(re.escape(name) for name in names) + r")-[0-9a-f]+\.(?:exe|pdb)")
    for path in sorted(deps.iterdir()):
        if pattern.fullmatch(path.name) is None:
            continue
        info = path.lstat()
        require(stat.S_ISREG(info.st_mode) and not info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT,
                "Cleanup candidate is not a plain file")
        require(path.resolve(strict=True).parent == deps and path.parent == deps, "Cleanup leaf escaped deps")
        again = path.lstat()
        require((again.st_dev, again.st_ino, again.st_size, again.st_mtime_ns) ==
                (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns), "Cleanup candidate changed")
        path.unlink()
        removed.append({"name": path.name, "bytes": info.st_size})
    return removed


class PhaseRun:
    def __init__(self, output, manifest, freeze, ordinary):
        self.output, self.manifest, self.freeze, self.ordinary = output, manifest, freeze, ordinary
        self.results = []
        self.coverage = {}

    def frozen(self):
        require(source_manifest() == self.manifest, "Source inventory or bytes changed during acceptance")
        require(digest(RUNNER.read_bytes()) == self.freeze["runnerSha256"], "Runner changed during acceptance")

    def run(self, label, command, temp=None, inventory=False):
        self.frozen()
        log = self.output / (label + ".log")
        require(not log.exists(), "Refusing to overwrite evidence log")
        started = time.monotonic()
        chosen_temp = temp or self.ordinary
        print(json.dumps({"running": label, "log": str(log), "tempRoot": str(chosen_temp)}), flush=True)
        with log.open("xb") as stream:
            status = subprocess.run(["rtk", "proxy", *command], cwd=ROOT,
                                    env=dict(os.environ, TEMP=str(chosen_temp), TMP=str(chosen_temp), CARGO_TARGET_DIR=str(BUILD),
                                             PYTHONUTF8="1", NO_COLOR="1", CARGO_TERM_COLOR="never"),
                                    stdout=stream, stderr=subprocess.STDOUT).returncode
        data = log.read_bytes()
        summaries = [match for line in data.decode("utf-8", errors="replace").splitlines()
                     if (match := TEST_END.fullmatch(line)) is not None]
        result = {"label": label, "command": command, "exitCode": status,
                  "seconds": round(time.monotonic() - started, 3), "logSha256": digest(data),
                  "origin": "fresh_execution", "tempRoot": str(chosen_temp),
                  "sourceManifestSha256": self.freeze["sourceManifestSha256"],
                  "runnerSha256": self.freeze["runnerSha256"], "inventoryOnly": inventory,
                  "groups": len(summaries),
                  **{key: sum(int(match[index]) for match in summaries)
                     for key, index in (("passed", 2), ("failed", 3), ("ignored", 4))}}
        self.results.append(result)
        self.frozen()
        print(json.dumps(result), flush=True)
        return result, data

    def inventories(self, label, selectors, names, allow_empty=False):
        all_result, all_data = self.run(label + "-all", BASE + selectors + ["--", "--list"], inventory=True)
        ignored_result, ignored_data = self.run(label + "-ignored", BASE + selectors + ["--", "--ignored", "--list"], inventory=True)
        require(all_result["exitCode"] == ignored_result["exitCode"] == 0, "Cargo inventory command failed")
        all_groups = group_map(harness_groups(all_data, True), names)
        ignored_groups = group_map(harness_groups(ignored_data, True), names)
        records = {}
        for name in names:
            registered = sorted(all_groups[name]["rows"])
            ignored = sorted(ignored_groups[name]["rows"])
            require(set(ignored).issubset(registered), "Ignored inventory contains unregistered names")
            require(allow_empty or registered, "Zero registered tests selected for " + name)
            require(all_groups[name]["executable"] == ignored_groups[name]["executable"], "Inventory executable drift")
            records[name] = {"registered": registered, "ignored": ignored,
                             "executable": all_groups[name]["executable"]}
        write_json(self.output / (label + ".json"), records)
        return records

    def execute(self, label, selectors, records, selected=None, temp=None, extra=(), cached=False, threads=4):
        require(threads in (1, 4), "Unsupported fixture scheduling profile")
        result, data = self.run(label, BASE + selectors + ["--", "--format", "pretty", "--test-threads=" + str(threads), *extra], temp)
        actual = group_map(harness_groups(data, False), list(records))
        if cached:
            require(re.search(rb"^\s*Compiling\s", data, re.MULTILINE) is None, "ACL fixture unexpectedly rebuilt instead of using cached Cargo")
        result["testGroups"] = []
        for name, record in records.items():
            expected = set(record["registered"] if selected is None else selected[name])
            require(expected.issubset(record["registered"]), "Unregistered selection")
            group = actual[name]
            require(group["executable"] == record["executable"], "Executed Cargo artifact differs from inventory")
            require(set(group["rows"]) == expected, "Executed test names differ from explicit selection")
            require({key for key, value in group["rows"].items() if value == "ignored"} == expected.intersection(record["ignored"]),
                    "Ignored result truth differs from registered ignored inventory")
            require(group["filtered"] == len(record["registered"]) - len(expected), "Filtered count differs from selection")
            result["testGroups"].append(dict(target=name, **group))
        result.update(groups=len(actual), **{key: sum(group[key] for group in actual.values()) for key in ("passed", "failed", "ignored")})
        require(result["passed"] + result["failed"] + result["ignored"] == sum(len(group["rows"]) for group in actual.values()),
                "Executed group total mismatch")
        return result, actual

    def native(self, acl):
        metadata_result, data = self.run("cargo-metadata", ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"], inventory=True)
        require(metadata_result["exitCode"] == 0, "Cargo metadata failed")
        metadata = json.loads(data)
        packages = [package for package in metadata["packages"] if package["id"] in metadata["workspace_members"]]
        require(len(packages) == 1 and packages[0]["name"] == "yoke-code-intelligence", "Workspace package drift requires explicit inventory review")
        targets = packages[0]["targets"]
        require(all(target["kind"] in [[kind] for kind in ("lib", "bin", "test", "example", "bench")] for target in targets),
                "Unexpected Cargo target kind")
        write_json(self.output / "target-inventory.json", targets)
        by_kind = {kind: sorted(target["name"] for target in targets if target["kind"] == [kind])
                   for kind in ("lib", "bin", "test", "example", "bench")}
        require(len(by_kind["lib"]) == 1 and by_kind["test"], "Missing library or integration targets")
        require(all(len(names) == len(set(names)) for names in by_kind.values()), "Duplicate Cargo target names")
        deps = Path(metadata["target_directory"]).resolve() / "debug" / "deps"
        require(deps == BUILD / "debug" / "deps", "Unexpected Cargo build-cache directory refused")
        # E: has capacity for all generated binaries. Cargo checks its build
        # fingerprints on each fresh inventory/execution; retain those outputs
        # instead of forcing every integration target to relink for each gate.
        write_json(self.output / "cache-policy.json", {"targetDirectory": str(BUILD),
                   "generatedBinariesRetained": True, "cargoFingerprintChecks": True})
        library = self.inventories("library-inventory", ["--lib"], by_kind["lib"])
        lib_name, record = next(iter(library.items()))
        registered, ignored = set(record["registered"]), set(record["ignored"])
        require(set(ACL_TESTS).issubset(registered) and not set(ACL_TESTS).intersection(ignored), "ACL fixture name/ignore drift")
        require(IGNORED_WORKER in registered and IGNORED_WORKER in ignored, "Owned worker must remain registered and ignored")
        # Libtest --skip is substring based. Prove those exact selectors cannot
        # filter a fourth name before using them on the ordinary D: partition.
        require({name for name in registered if any(skip in name for skip in ACL_TESTS)} == set(ACL_TESTS), "Ambiguous ACL skip selectors")
        ordinary_names = registered.difference(ACL_TESTS)
        require(ordinary_names, "Zero ordinary library tests selected")
        ordinary, ordinary_groups = self.execute("library-e", ["--lib"], library,
            {lib_name: ordinary_names}, extra=[value for name in ACL_TESTS for value in ("--skip", name)])
        partitions = [{"label": ordinary["label"], "names": sorted(ordinary_groups[lib_name]["rows"])}]
        for index, name in enumerate(ACL_TESTS):
            exact, groups = self.execute("library-c-acl-" + str(index), ["--lib", name], library,
                {lib_name: {name}}, temp=acl, extra=["--exact"], cached=True)
            require(exact["groups"] == 1 and exact["passed"] + exact["failed"] == 1 and exact["ignored"] == 0, "Zero or ignored exact ACL fixture selection")
            partitions.append({"label": exact["label"], "names": sorted(groups[lib_name]["rows"])})
        covered = [name for partition in partitions for name in partition["names"]]
        require(len(covered) == len(set(covered)) == len(registered) and set(covered) == registered,
                "Library E/C partitions overlap or omit registered tests")
        self.coverage.update(libraryRegistered=len(registered), libraryIgnored=len(ignored),
                             libraryIgnoredNames=sorted(ignored), ignoredWorker=IGNORED_WORKER,
                             libraryPartitions=partitions, libraryDisjointCoverageAccepted=True)
        for kind, selector in (("bin", "--bins"), ("example", "--examples"), ("bench", "--benches")):
            if by_kind[kind]:
                records = self.inventories(kind + "-inventory", [selector], by_kind[kind], allow_empty=True)
                self.execute(kind + "-tests", [selector], records)
            else:
                self.coverage[kind + "Targets"] = []
        docs, doc_data = self.run("doctests", BASE + ["--doc", "--", "--test-threads=4"])
        # Rustdoc uses its own 'Doc-tests' header, not Cargo's Running header.
        doc_groups = harness_groups(re.sub(rb"^\s*Doc-tests (\S+)\r?$", rb" Running doctests (\1.exe)", doc_data, flags=re.MULTILINE), False)
        require(len(doc_groups) == 1, "Unexpected or missing documentation harness")
        docs.update(groups=len(doc_groups), **{key: sum(group[key] for group in doc_groups) for key in ("passed", "failed", "ignored")})
        docs["testGroups"] = doc_groups
        names = by_kind["test"]
        selectors = [value for name in names for value in ("--test", name)]
        require(len(subprocess.list2cmdline(BASE + selectors)) < 30_000, "Integration command exceeds Windows argument bound")
        # Compile and inventory every integration binary once on capacious E:.
        # Preserve each independent harness/executable/name/ignore check. Retain
        # generated binaries on E: rather than repeatedly linking deleted files.
        records = self.inventories("integration-inventory", selectors, names)
        special = "ignored_capture"
        require(special in records, "Short-name integration target missing")
        short_tests = {
            "windows_short_name_aliases_cannot_create_files_inside_host_readonly_prefixes",
            "windows_short_name_aliases_do_not_grant_protected_writes_without_ignored_optin",
        }
        require(short_tests.issubset(records[special]["registered"])
                and not short_tests.intersection(records[special]["ignored"]),
                "Required real short-name tests missing or ignored")
        serial = "query_parity"
        callback_test = "public_engine_semantics_preserve_host_configuration_callbacks"
        require(serial in records and callback_test in records[serial]["registered"]
                and callback_test not in records[serial]["ignored"], "Required callback fixture missing or ignored")
        serial_hosts = {
            "github": "explicit_loopback_http_reads_observed_ci_review_and_scope_without_commands",
            "debug_adapter": "reverse_launch_authority_and_foreign_stale_or_unstopped_frames_are_refused",
        }
        require(set(serial_hosts).issubset(records), "Required serial host targets missing")
        require(all(test in records[name]["registered"] and test not in records[name]["ignored"]
                    for name, test in serial_hosts.items()), "Required serial host cases missing or ignored")
        ordinary = [name for name in names if name not in (special, serial) and name not in serial_hosts]
        require(ordinary, "No ordinary integration targets")
        self.execute("integration-e", [value for name in ordinary for value in ("--test", name)],
                     {name: records[name] for name in ordinary})
        # This target includes both a 36-MB corpus workload and a one-second
        # callback assertion. Serialize its tests without extending deadlines.
        self.execute("integration-e-serial-query", ["--test", serial], {serial: records[serial]},
                     threads=1, cached=True)
        self.execute("integration-c-shortnames", ["--test", special], {special: records[special]},
                     temp=acl, cached=True)
        # Keep host protocol assertions under their original deadlines while
        # avoiding concurrent fixture startups inside these two executables.
        for target in sorted(serial_hosts):
            self.execute("integration-e-serial-" + target, ["--test", target],
                         {target: records[target]}, threads=1, cached=True)
        integration_selected = ordinary + [serial, special] + sorted(serial_hosts)
        require(sorted(integration_selected) == names and len(integration_selected) == len(set(integration_selected)),
                "Metadata integration targets were omitted or executed twice")
        self.coverage.update(integrationTargets=len(integration_selected), integrationTargetNames=integration_selected,
                             integrationCoverageAccepted=True, targetNamesByKind=by_kind,
                             integrationPartitions={"e": ordinary, "eSerialQuery": [serial], "cShortNames": [special], "eSerialHostProtocols": sorted(serial_hosts)},
                             serialQueryRequiredTest=callback_test,
                             requiredRealShortNameTests=sorted(short_tests))

    def static(self, phase):
        if phase == "format":
            self.run("format", ["cargo", "fmt", "--all", "--", "--check"])
        elif phase == "clippy":
            self.run("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "-j", "1", "--", "-D", "warnings"])
        elif phase == "python":
            result, data = self.run("python", ["python", "-X", "utf8", "-m", "unittest", "discover", "-s", "scripts/tests", "-v"])
            text = "\n".join(data.decode("utf-8").splitlines())
            require(re.findall(r"^Ran (\d+) tests? in .+$", text, re.MULTILINE) == ["11"], "Python test-count drift")
            names = re.findall(r"^(test_\S+ \([^\n]+\)) \.\.\. ok$", text, re.MULTILINE)
            require(len(names) == len(set(names)) == 11 and re.search(r"^OK$", text, re.MULTILINE), "Python result-line/count mismatch")
            result.update(groups=1, passed=11, testNames=names)
        elif phase == "typescript":
            self.run("typecheck", ["npm.cmd", "run", "typecheck"])
            result, data = self.run("typescript", ["npm.cmd", "test"])
            values = re.findall(r"^(?:#|\u2139)\s+(tests|pass|fail|cancelled|skipped|todo)\s+(\d+)\s*$", data.decode("utf-8"), re.MULTILINE)
            require(len(values) == 6 and len(dict(values)) == 6, "Missing or duplicate Node test summary")
            counts = {key: int(value) for key, value in values}
            require(counts["pass"] > 0 and counts["cancelled"] == counts["todo"] == 0, "Zero selected or incomplete Node tests")
            require(counts["tests"] == counts["pass"] + counts["fail"] + counts["skipped"], "Node summary total mismatch")
            result.update(groups=1, passed=counts["pass"], failed=counts["fail"], ignored=counts["skipped"], frameworkCounts=counts)


def finalize(bundle, manifest, freeze):
    receipts = {}
    for phase in PHASES:
        output = bundle / phase
        receipt = load_json(output / "receipt.json")
        require(receipt["phase"] == phase and receipt["accepted"] and not receipt["sourceChanges"], "Unaccepted phase: " + phase)
        require(receipt["sourceManifestSha256"] == freeze["sourceManifestSha256"] and receipt["runnerSha256"] == freeze["runnerSha256"], "Cross-phase freeze identity differs")
        require(load_json(output / "source-before.json") == load_json(output / "source-after.json") == manifest, "Cross-phase source manifests differ")
        require(receipt["results"] and all(result["exitCode"] == 0 and result["failed"] == 0 and result["origin"] == "fresh_execution" for result in receipt["results"]), "Phase has unaccepted or retained results")
        require(all(result["sourceManifestSha256"] == freeze["sourceManifestSha256"] and
                    result["runnerSha256"] == freeze["runnerSha256"] for result in receipt["results"]),
                "A phase result has a different source or runner identity")
        require(all(receipt[key] == sum(result[key] for result in receipt["results"])
                    for key in ("groups", "passed", "failed", "ignored")), "Phase result totals differ")
        for relative, expected in receipt["artifactSha256"].items():
            path = output / relative
            require(path.parent == output and digest(path.read_bytes()) == expected, "Phase artifact hash differs: " + relative)
        for result in receipt["results"]:
            require(digest((output / (result["label"] + ".log")).read_bytes()) == result["logSha256"], "Result log hash differs")
        receipts[phase] = {"receiptSha256": digest((output / "receipt.json").read_bytes()),
                           **{key: receipt[key] for key in ("passed", "failed", "ignored", "groups")}}
    summary = dict(checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(), accepted=True,
                   **freeze, phaseReceipts=receipts,
                   freezeArtifactSha256={name: digest((bundle / name).read_bytes())
                                         for name in ("freeze.json", "source-freeze.json", "runner.py")},
                   interpretation="All five phases freshly executed on identical frozen source and runner; ignored tests remain ignored. Three disjoint ACL fixtures and the complete ignored_capture target execute on C. query_parity, github and debug_adapter execute serially on E with unchanged deadlines. Other ordinary integration targets execute on E with four threads; real short-name fixtures execute on C.")
    write_json(bundle / "summary.json", summary)
    print(json.dumps(summary), flush=True)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=(*PHASES, "finalize"))
    parser.add_argument("--tag", required=True, help="Reuse one shared tag for all five phases")
    parser.add_argument("--temp", help="Fresh ordinary fixture root on E:")
    parser.add_argument("--acl-temp", help="Native only: fresh tiny ACL fixture root on C:")
    args = parser.parse_args()
    require(os.name == "nt" and ROOT.name == "Code-Intelligence" and (ROOT / "native/Cargo.toml").is_file(), "Unexpected Windows source root")
    bundle = checked_tag(args.tag)
    if args.phase == "finalize":
        require(bundle.exists() and args.temp is None and args.acl_temp is None, "Finalize requires an existing tag and no fixture arguments")
        manifest, freeze = open_freeze(bundle)
        return finalize(bundle, manifest, freeze)
    require(args.phase == "native" or args.acl_temp is None, "ACL temp is native-only")
    output = bundle / args.phase
    require(not output.exists(), "Refusing to overwrite a completed or failed phase; choose a new tag")
    ordinary = new_temp(args.temp, "E")
    acl = new_temp(args.acl_temp, "C") if args.phase == "native" else None
    manifest, freeze = open_freeze(bundle)
    output.mkdir()
    write_json(output / "source-before.json", manifest)
    run = PhaseRun(output, manifest, freeze, ordinary)
    errors = []
    try:
        if args.phase == "native":
            run.native(acl)
        else:
            run.static(args.phase)
    except Exception as error:
        errors.append(type(error).__name__ + ": " + str(error))
    after = source_manifest()
    write_json(output / "source-after.json", after)
    changed = sorted(name for name in manifest.keys() | after.keys() if manifest.get(name) != after.get(name))
    if digest(RUNNER.read_bytes()) != freeze["runnerSha256"]:
        errors.append("Runner changed during phase")
    artifacts = {path.name: digest(path.read_bytes()) for path in sorted(output.iterdir()) if path.is_file()}
    receipt = dict(checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(), phase=args.phase,
                   **freeze, sourceChanges=changed, validationErrors=errors, results=run.results,
                   **{key: sum(result[key] for result in run.results) for key in ("groups", "passed", "failed", "ignored")},
                   matrixCoverage=run.coverage, tempRoot=str(ordinary), aclTempRoot=str(acl) if acl else None,
                   artifactSha256=artifacts,
                   accepted=not errors and not changed and bool(run.results) and all(result["exitCode"] == 0 and result["failed"] == 0 for result in run.results),
                   interpretation="Fresh execution only. Shared freeze required across all five phases; ignored test results are never relabeled as passing.")
    write_json(output / "receipt.json", receipt)
    print(json.dumps(receipt), flush=True)
    return 0 if receipt["accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
