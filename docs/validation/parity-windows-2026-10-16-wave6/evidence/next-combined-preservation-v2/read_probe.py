"""Tiny actual-read timing and sentinel checks; no preservation plan or application."""
import ast
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile
import time
from unittest.mock import patch

sys.dont_write_bytecode = True
HERE = Path(__file__).absolute().parent


def main():
    candidate = HERE / "preserve_combined.py"
    before_path = HERE / "guard-only-before.py"
    ast.parse(candidate.read_bytes())
    current, before = runpy.run_path(str(candidate)), runpy.run_path(str(before_path))
    old_raw = current["CORE_PATH"].read_bytes()
    assert hashlib.sha256(old_raw).hexdigest() == current["CORE_SHA"]
    original = {}
    exec(compile(old_raw, str(current["CORE_PATH"]), "exec"), original)
    paths = ("acceptance_wave4.py", "r5-final-public-20261016-1212/run.log",
             "next-git-impact-details/revisions/normal-final-promotion-v3/apply_normal.py")
    rows = []
    for repeat in range(4):
        for name in paths:
            path = current["EVIDENCE"] / name
            row, payloads = dict(repeat=repeat, path=name), {}
            variants = (("oldCore", original["read"]), ("v2LargeRequest", before["read"]), ("v2BoundedRequest", current["read"]))
            for label, reader in variants if repeat % 2 == 0 else reversed(variants):
                started = time.perf_counter_ns()
                payloads[label] = reader(path, current["EVIDENCE"])
                row[label + "Milliseconds"] = (time.perf_counter_ns() - started) / 1e6
            assert payloads["oldCore"] == payloads["v2LargeRequest"] == payloads["v2BoundedRequest"]
            row.update(bytes=len(payloads["oldCore"]), sha256=hashlib.sha256(payloads["oldCore"]).hexdigest())
            rows.append(row)
    fixture = Path(tempfile.mkdtemp(prefix="read-smoke-", dir=HERE))
    file = fixture / "source.bin"
    data = b"tiny exact fixture\n"
    file.write_bytes(data)
    real_open, reader, checks, requests = type(file).open, current["read"], [], []
    class ObservedStream:
        def __init__(self, stream, mode): self.stream, self.mode = stream, mode
        def __enter__(self): self.stream.__enter__(); return self
        def __exit__(self, *args): return self.stream.__exit__(*args)
        def fileno(self): return self.stream.fileno()
        def read(self, request):
            requests.append(request)
            if self.mode == "grow": return data + b"x"
            if self.mode == "shrink": return data[:-1]
            return self.stream.read(request)
    def observed(mode):
        def opened(path, *args, **kwargs):
            stream = real_open(path, *args, **kwargs)
            return ObservedStream(stream, mode) if path == file else stream
        return opened
    with patch.object(type(file), "open", observed("exact")):
        assert reader(file, HERE) == data
    assert requests == [len(data) + 1]
    checks.append("actual_tiny_read_requests_exact_size_plus_one")
    for mode in ("grow", "shrink"):
        try:
            with patch.object(type(file), "open", observed(mode)):
                reader(file, HERE)
        except RuntimeError:
            checks.append("sentinel_" + mode + "_refused")
        else:
            raise AssertionError("Changed length admitted")
    with patch.object(current["core"], "MAX_FILE", len(data) - 1):
        try:
            reader(file, HERE)
        except RuntimeError:
            checks.append("oversize_preopen_refused")
        else:
            raise AssertionError("Byte cap widened")
    real_fstat, counter = os.fstat, [0]
    def changed_after_read(handle):
        info = real_fstat(handle); counter[0] += 1
        if counter[0] == 2:
            import types
            return types.SimpleNamespace(st_dev=info.st_dev, st_ino=info.st_ino + 1,
                                         st_size=info.st_size, st_mtime_ns=info.st_mtime_ns)
        return info
    with patch.object(os, "fstat", changed_after_read):
        try:
            reader(file, HERE)
        except RuntimeError:
            checks.append("completed_fstat_identity_mismatch_refused")
        else:
            raise AssertionError("Post-read identity mismatch admitted")
    file.write_bytes(b"")
    requests.clear()
    with patch.object(type(file), "open", observed("exact")):
        assert reader(file, HERE) == b""
    assert requests == [1]
    checks.append("empty_file_requests_one_sentinel_byte")
    totals = {label: sum(row[label + "Milliseconds"] for row in rows)
              for label in ("oldCore", "v2LargeRequest", "v2BoundedRequest")}
    result = dict(scope="three_tiny_actual_paths_four_alternating_rounds_not_full_plan",
                  checkedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  candidateSha256=hashlib.sha256(candidate.read_bytes()).hexdigest(),
                  guardOnlyBeforeSha256=hashlib.sha256(before_path.read_bytes()).hexdigest(),
                  frozenCoreSha256=current["CORE_SHA"], rounds=4, totalsMilliseconds=totals,
                  sameGuardAllocationSpeedup=totals["v2LargeRequest"] / totals["v2BoundedRequest"],
                  oldCoreToFinalSpeedup=totals["oldCore"] / totals["v2BoundedRequest"],
                  rows=rows, checks=checks, fixtureRetained=fixture.name,
                  normalWrites=False, fullPlanOrApply=False, gatesInvoked=False, cacheOrProcessChanges=False,
                  interpretation="Measured tiny-file read improvement only; full preservation wall time and total-memory behavior remain unmeasured.")
    with (HERE / "read-probe-results.json").open("xb") as stream:
        stream.write((json.dumps(result, indent=2, sort_keys=True) + "\n").encode("utf-8"))
    print(json.dumps({key: result[key] for key in ("totalsMilliseconds", "sameGuardAllocationSpeedup", "oldCoreToFinalSpeedup")}))
    print("Tiny IO checks PASS: " + str(len(checks)) + "; no full plan/application/gates/normal/cache/process changes.")


if __name__ == "__main__":
    main()
