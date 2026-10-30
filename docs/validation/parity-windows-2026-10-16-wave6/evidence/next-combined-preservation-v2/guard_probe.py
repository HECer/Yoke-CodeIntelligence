"""Bounded paired guard timings and small IO checks; never plan/apply or run gates."""
import argparse
import ast
import ctypes
import datetime
import hashlib
import json
import os
from pathlib import Path
import runpy
import stat
import sys
import tempfile
import time
import types
from unittest.mock import patch

sys.dont_write_bytecode = True
HERE = Path(__file__).absolute().parent
PATHS = (
    "acceptance_wave4.py",
    "r5-final-public-20261016-1212/run.log",
    "next-git-impact-details/revisions/normal-final-promotion-v3/apply_normal.py",
    "next-git-impact-details/revisions/combined-context-prep-v1/actual-contexts/combined-r3-r5-20261016-1315.json",
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results-out", required=True)
    args = parser.parse_args()
    output = Path(args.results_out)
    assert output.is_absolute() and output.parent == HERE and output.name == "guard-probe-results.json" and not output.exists()
    candidate = HERE / "preserve_combined.py"
    ast.parse(candidate.read_bytes())
    current = runpy.run_path(str(candidate))
    evidence = current["EVIDENCE"]
    core_raw = current["CORE_PATH"].read_bytes()
    assert hashlib.sha256(core_raw).hexdigest() == current["CORE_SHA"]
    old = types.ModuleType("probe_unmodified_core")
    exec(compile(core_raw, str(current["CORE_PATH"]), "exec"), old.__dict__)
    assert all((evidence / name).is_file() for name in PATHS)
    rows = []
    for repeat in range(8):
        for name in PATHS:
            path = evidence / name
            row = dict(repeat=repeat, path=name, depth=len(Path(name).parts),
                       sha256=hashlib.sha256(path.read_bytes()).hexdigest())
            guards = (("old", old.plain), ("v2", current["plain"]))
            for label, guard in guards if repeat % 2 == 0 else reversed(guards):
                started = time.perf_counter_ns()
                assert guard(path, evidence, file=True) == path
                row[label + "Milliseconds"] = (time.perf_counter_ns() - started) / 1e6
            rows.append(row)
    checks = []
    def rejected(label, operation):
        try:
            operation()
        except (RuntimeError, OSError):
            checks.append(label)
        else:
            raise AssertionError("Guard admitted " + label)
    plain, read = current["plain"], current["read"]
    fixture = Path(tempfile.mkdtemp(prefix="guard-smoke-", dir=HERE))
    regular = fixture / "source.bin"
    regular.write_bytes(b"bounded guard fixture\n")
    assert read(regular, HERE) == regular.read_bytes()
    checks.append("existing_regular_file_read")
    absent = fixture / "absent" / "leaf.json"
    assert plain(absent, HERE, file=True, missing=True) == absent
    checks.append("missing_tail_allowed_with_existing_prefix_strict")
    rejected("missing_tail_refused_without_missing_flag", lambda: plain(absent, HERE, file=True))
    rejected("directory_as_file", lambda: plain(fixture, HERE, file=True))
    rejected("file_as_directory", lambda: plain(regular, HERE))
    rejected("boundary_self", lambda: plain(HERE, HERE))
    rejected("outside_boundary", lambda: plain(HERE.parent / "outside.json", HERE, file=True, missing=True))
    rejected("parent_traversal", lambda: plain(fixture / ".." / "missing.json", HERE, file=True, missing=True))
    # Core lookups and local aliases must use the same guarded functions.
    assert current["core"].plain is plain and current["core"].read is read
    assert current["core"].load.__globals__["read"] is read
    assert current["core"].validate_bundle.__globals__["plain"] is plain
    checks.append("all_core_and_local_io_aliases_injected")
    actual_entries = current["entries"](evidence)
    assert evidence / "r5-final-public-20261016-1212" in actual_entries
    checks.append("actual_direct_evidence_enumeration")
    rejected("arbitrary_boundary_self_enumeration", lambda: current["entries"](fixture, fixture))
    # A deterministic substitute verifies the Windows reparse flag when symlink
    # creation is unavailable. Real lstat is retained on every other component.
    real_lstat = type(regular).lstat
    def flagged(path):
        info = real_lstat(path)
        if path == fixture:
            return types.SimpleNamespace(st_mode=info.st_mode, st_file_attributes=0x400)
        return info
    with patch.object(type(regular), "lstat", flagged):
        rejected("synthetic_reparse_ancestor", lambda: plain(regular, HERE, file=True))
    actual_reparse = "unavailable"
    link = fixture / "reparse-link"
    try:
        os.symlink(str(fixture), str(link), target_is_directory=True)
    except OSError as error:
        actual_reparse = "symlink_creation_unavailable_winerror_" + str(getattr(error, "winerror", None))
    else:
        rejected("actual_reparse_ancestor", lambda: plain(link / regular.name, HERE, file=True))
        actual_reparse = "actual_symlink_refused"
    # Read-only OS alias discovery: do not change volume 8.3 policy.
    actual_alias = []
    for path, boundary in ((evidence / PATHS[-1], evidence), (Path(sys.executable), Path(sys.executable).anchor)):
        boundary = Path(boundary)
        buffer = ctypes.create_unicode_buffer(32768)
        count = ctypes.windll.kernel32.GetShortPathNameW(str(path), buffer, len(buffer))
        if 0 < count < len(buffer) and Path(buffer.value) != path:
            alias = Path(buffer.value)
            # Keep a lexical canonical boundary; a whole-drive alias is never an authority grant.
            if alias.is_relative_to(boundary):
                rejected("actual_8dot3_alias", lambda: plain(alias, boundary, file=True))
                actual_alias.append(dict(path=str(path), alias=str(alias), refused=True))
    # Open and post-read identity mismatches must refuse despite unchanged size.
    original_fstat = os.fstat
    def mismatched(handle):
        info = original_fstat(handle)
        return types.SimpleNamespace(st_dev=info.st_dev, st_ino=info.st_ino + 1,
                                     st_size=info.st_size, st_mtime_ns=info.st_mtime_ns)
    with patch.object(os, "fstat", mismatched):
        rejected("opened_identity_mismatch", lambda: read(regular, HERE))
    calls = []
    def resolve_failure(path, strict=False):
        calls.append((path, strict))
        if path == fixture:
            raise PermissionError("synthetic existing-prefix sharing/access refusal")
        return real_resolve(path, strict=strict)
    real_resolve = type(regular).resolve
    with patch.object(type(regular), "resolve", resolve_failure):
        rejected("missing_tail_existing_prefix_resolve_failure", lambda: plain(absent, HERE, file=True, missing=True))
    assert all(strict is True for _, strict in calls)
    checks.append("all_missing_tail_resolution_remains_strict")
    old_ms = sum(row["oldMilliseconds"] for row in rows)
    new_ms = sum(row["v2Milliseconds"] for row in rows)
    result = dict(scope="bounded_guard_probe_not_full_preservation_or_runtime_suite",
                  checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  candidateSha256=hashlib.sha256(candidate.read_bytes()).hexdigest(),
                  frozenCoreSha256=current["CORE_SHA"], rounds=8, alternatingOldNewOrder=True,
                  oldTotalMilliseconds=old_ms, v2TotalMilliseconds=new_ms, speedup=old_ms / new_ms,
                  rows=rows, checks=checks, actualReparse=actual_reparse, actual8dot3Aliases=actual_alias,
                  fixtureRetained=fixture.name, directEvidenceEntries=len(actual_entries),
                  normalWrites=False, fullPlanOrApply=False, gatesInvoked=False,
                  cacheOrProcessChanges=False,
                  interpretation="Per-guard timings only; no claim about total preservation duration or the cause of root's earlier slow plan.")
    with output.open("xb") as stream:
        stream.write((json.dumps(result, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    print(json.dumps({key: result[key] for key in ("oldTotalMilliseconds", "v2TotalMilliseconds", "speedup", "actualReparse", "directEvidenceEntries")}))
    print("Bounded guard checks PASS: " + str(len(checks)) + "; actual 8.3 aliases refused: " + str(len(actual_alias)))
    print("No full plan, application, gates, normal/cache/process changes.")


if __name__ == "__main__":
    main()
