"""Exercise only acceptance parsers against immutable historical log bytes."""
import argparse
import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
RUNNER = HERE / "acceptance_wave3.py"
HISTORICAL = HERE / "native-wave2-matrix"
PYTHON_LOG = HERE / "python-wave2-1440" / "python.log"
NODE_LOG = HERE / "typescript-wave2-1440" / "typescript.log"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    output = Path(args.output).resolve()
    if output.parent != HERE or output.exists():
        raise RuntimeError("Oracle output must be a fresh sibling evidence file")
    runner_bytes = RUNNER.read_bytes()
    ast.parse(runner_bytes, filename=str(RUNNER))
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("acceptance_wave3_parser_only", RUNNER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # __main__ is not entered.

    def forbidden(*args, **kwargs):
        raise RuntimeError("Subprocess execution is forbidden in the parser oracle")

    module.subprocess.run = forbidden
    module.subprocess.check_output = forbidden
    paths = [PYTHON_LOG, NODE_LOG, HISTORICAL / "lib-test-inventory.log",
             HISTORICAL / "package.log", HISTORICAL / "doctests.log",
             HERE / "acceptance.py", *sorted(HISTORICAL.glob("integration-*.log"))]
    original = {path: path.read_bytes() for path in paths}
    cases = []

    def case(name, callback, reject=False):
        try:
            detail = callback()
        except Exception as error:
            cases.append(dict(name=name, accepted=reject, expectedRejection=reject,
                              error=type(error).__name__ + ": " + str(error)))
        else:
            cases.append(dict(name=name, accepted=not reject, expectedRejection=reject, detail=detail))

    def static(phase, data):
        instance = module.PhaseRun(HERE, {}, {}, HERE)
        captured = []

        def fixture_run(label, command, *args, **kwargs):
            result = dict(label=label, command=command, exitCode=0, groups=0, passed=0, failed=0, ignored=0)
            captured.append(result)
            return result, data if label in ("python", "typescript") else b""

        instance.run = fixture_run
        instance.static(phase)
        result = captured[-1]
        expected = (11, 0) if phase == "python" else (240, 3)
        module.require((result["passed"], result["ignored"]) == expected and result["failed"] == 0,
                       "Historical parser result differs")
        return {key: result[key] for key in ("passed", "failed", "ignored", "groups")}

    python = original[PYTHON_LOG]
    node = original[NODE_LOG]
    case("actual-python-11-crlf", lambda: static("python", python))
    case("python-11-lf-view", lambda: static("python", python.replace(b"\r\n", b"\n")))
    case("python-11-crlf-view", lambda: static("python", b"\r\n".join(python.splitlines()) + b"\r\n"))
    case("actual-node-240-3", lambda: static("typescript", node))
    case("node-240-3-crlf-view", lambda: static("typescript", b"\r\n".join(node.splitlines()) + b"\r\n"))
    first_python_row = next(line for line in python.splitlines() if line.startswith(b"test_"))
    case("python-missing-result-rejected", lambda: static("python", python.replace(first_python_row, b"", 1)), reject=True)
    case("python-duplicate-result-rejected", lambda: static("python", python + first_python_row + b"\n"), reject=True)
    case("node-total-mismatch-rejected", lambda: static("typescript", re.sub(rb"(tests\s+)243", rb"\g<1>244", node)), reject=True)

    listing = original[HISTORICAL / "lib-test-inventory.log"]
    registered = module.group_map(module.harness_groups(listing, True), ["yoke"])["yoke"]
    case("actual-native-library-list-97", lambda: module.require(len(registered["rows"]) == 97, "Library inventory count differs"))
    case("native-library-list-crlf-view", lambda: module.require(
        module.harness_groups(b"\r\n".join(listing.splitlines()) + b"\r\n", True) == module.harness_groups(listing, True), "List CRLF grammar differs"))
    package = original[HISTORICAL / "package.log"]
    groups = module.harness_groups(package, False)
    selected = set(registered["rows"]).difference(module.ACL_TESTS[:2])
    case("actual-native-library-selected-names-95", lambda: module.require(
        set(groups[0]["rows"]) == selected and groups[0]["passed"] == 95 and groups[0]["filtered"] == 2,
        "Historical ordinary library partition differs"))
    case("native-package-crlf-view", lambda: module.require(
        module.harness_groups(b"\r\n".join(package.splitlines()) + b"\r\n", False) == groups, "Result CRLF grammar differs"))
    for path in sorted(HISTORICAL.glob("integration-*.log")):
        def check_integration(path=path):
            parsed = module.harness_groups(original[path], False)
            module.require(parsed and all(group["failed"] == 0 for group in parsed), "Historical integration grammar differs")
            return dict(groups=len(parsed), passed=sum(group["passed"] for group in parsed),
                        ignored=sum(group["ignored"] for group in parsed))
        case("actual-" + path.stem, check_integration)
    docs = original[HISTORICAL / "doctests.log"]
    case("actual-doctest-header", lambda: module.require(len(module.harness_groups(
        re.sub(rb"^\s*Doc-tests (\S+)\r?$", rb" Running doctests (\1.exe)", docs, flags=re.MULTILINE), False)) == 1,
        "Documentation harness grammar differs"))
    first_native_row = next(line for line in package.splitlines() if line.startswith(b"test ") and b" ... ok" in line)
    case("native-missing-result-rejected", lambda: module.harness_groups(package.replace(first_native_row, b"", 1), False), reject=True)
    case("native-duplicate-result-rejected", lambda: module.harness_groups(package.replace(first_native_row, first_native_row + b"\n" + first_native_row, 1), False), reject=True)
    case("native-summary-count-drift-rejected", lambda: module.harness_groups(package.replace(b"95 passed;", b"96 passed;", 1), False), reject=True)
    case("native-unexpected-target-rejected", lambda: module.group_map(module.harness_groups(listing, True), ["foreign"]), reject=True)
    first_list_row = next(line for line in listing.splitlines() if line.endswith(b": test"))
    case("native-missing-inventory-row-rejected", lambda: module.harness_groups(listing.replace(first_list_row, b"", 1), True), reject=True)
    unchanged = all(path.read_bytes() == data for path, data in original.items()) and RUNNER.read_bytes() == runner_bytes
    report = dict(runnerSha256=sha(runner_bytes), oracleSha256=sha(Path(__file__).read_bytes()),
                  astParsed=True, runnerMainExecuted=False, subprocessesExecuted=0, suitesExecuted=0,
                  immutableInputsUnchanged=unchanged, cases=cases,
                  passed=sum(case["accepted"] for case in cases), failed=sum(not case["accepted"] for case in cases),
                  inputs={str(path.relative_to(HERE)): dict(sha256=sha(data), bytes=len(data), crlf=data.count(b"\r\n"))
                          for path, data in original.items()})
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: report[key] for key in ("runnerSha256", "oracleSha256", "astParsed", "runnerMainExecuted",
                                                 "subprocessesExecuted", "suitesExecuted", "immutableInputsUnchanged", "passed", "failed")}))
    for item in cases:
        if not item["accepted"]:
            print(json.dumps(item))
    return 0 if unchanged and report["failed"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
