"""Inert admission/orchestration tests; no native, browser, Job or network."""
import copy
import importlib.util
from pathlib import Path
import unittest
import subprocess
from types import SimpleNamespace
import tempfile
import io
from contextlib import redirect_stderr

SCRIPTS = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("usage_browser", SCRIPTS / "host_tool_usage_browser.py")
USAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(USAGE)


def require(value, message):
    if not value:
        raise RuntimeError(message)


def view():
    return {"schema": "native_tool_usage_display_v1", "estimator": "CHAR_COUNT",
            "scope": "current_session_instance_successful_serena_operations", "revision": "4",
            "hostGrantRemoteAttempts": "0", "tools": {
                "read_file": {"numTimesCalled": "2", "inputTokens": "14", "outputTokens": "23"}}}


def orchestrate(fault=None):
    events = []
    current = {**view(), "revision": "1", "tools": {
        "open_dashboard": {"numTimesCalled": "1", "inputTokens": "0", "outputTokens": "50"}}}
    exit_proof = {"browserCloseRequested": True, "browserExitCode": 0, "cdpEOF": True,
                  "browserOutputEOF": True, "driverExitCode": 0, "driverStdoutEOF": True,
                  "beforeForcedCleanup": True, "scriptErrors": 0, "consoleErrors": 0,
                  "statisticsResetRequests": {"desktop": 1, "mobile": 1}}
    read_result = {"status": "ok", "data": {"text": "owned \u03c0\U0001f980e\u0301\r\n" * 3}}
    output_tokens = len(USAGE.json.dumps(read_result["data"], ensure_ascii=False, separators=(",", ":"))) // 4
    wrapped_tokens = len(USAGE.json.dumps(read_result, ensure_ascii=False, separators=(",", ":"))) // 4
    require("\r\n" in read_result["data"]["text"] and output_tokens != len(USAGE.json.dumps(
        read_result["data"], ensure_ascii=False, separators=(",", ":")).encode()) // 4,
        "The inert reply must exercise actual Unicode scalars and CRLF")

    class Actor:
        def __init__(self, *args):
            events.append("actor-start")
            self.reads = 0

        def state(self):
            return {"view": {"activeProject": "default", "revision": 9,
                    "management": {"sessionStateSha256": ("changed" if fault == "identity" and self.reads == 3 else "a" * 64)},
                    "toolUsageStatisticsDisplay": copy.deepcopy(current)}}

        def rpc(self, method, args):
            require(method == "tools/call" and args == {"name": "read_file",
                    "arguments": {"relative_path": "main.rs"}}, "Unexpected tool")
            events.append("read")
            if fault == "actor-error":
                raise RuntimeError("Private capability and protocol body must stay private")
            self.reads += 1
            current["revision"] = str(int(current["revision"]) + 1)
            current["tools"]["read_file"] = {"numTimesCalled": str(self.reads), "inputTokens": str(self.reads * 6),
                                      "outputTokens": str(self.reads * (1 if fault == "output" else
                                          wrapped_tokens if fault == "output-wrapper" else output_tokens))}
            if fault == "text-fallback":
                return {"isError": False, "content": [{"type": "text", "text": USAGE.json.dumps(read_result)}]}
            return {"isError": False, "structuredContent": copy.deepcopy(read_result)}

        def shutdown(self):
            events.append("actor-natural-exit")
            return {"nativeExitCode": 0, "stdinWasStillOpen": True, "beforeJobCleanup": True}

        def close(self):
            events.append("actor-close")

    class Browser:
        def __init__(self, *args):
            self.start = {"variant": "tool-usage"}
            events.append("browser-start")

        def call(self, request):
            events.append(request["phase"] + request.get("viewport", ""))
            require(request["counts"] == current, "Oracle differs")
            if request["phase"] == "before":
                return {"states": ["desktop-before", "mobile-before"]}
            if request["phase"] == "pending":
                return {"states": ["mobile-pending", "desktop-pending"], "cancelNoRequest": fault != "cancel",
                        "expectedRevision": current["revision"]}
            if request["viewport"] == "desktop":
                return {"staleResetRefused": fault != "conflict", "resetRequests": 1}
            current.update(revision=str(int(current["revision"]) + 1), tools={})
            return {"explicitResetObserved": True, "resetRequests": 1}

        def shutdown(self):
            events.append("browser-natural-exit")
            if fault == "shutdown":
                raise RuntimeError("inert-shutdown-failure")
            return exit_proof

        def close(self):
            events.append("browser-close")

    def cleanup(browser, actors, runtimes):
        if browser:
            browser.close()
        for actor in actors:
            actor.close()

    harness = SimpleNamespace(require=require, LIVE=SimpleNamespace(Actor=Actor, MAX_JSON=1000000,
        decode=lambda raw: USAGE.json.loads(raw),
        require_worker_job=lambda name: events.append("job-admission")), Browser=Browser,
        write_json=lambda path, value: require(value["dashboard"]["management"]["toolUsageReset"] is True
            and value["tokenCountEstimator"] == "CHAR_COUNT", "Separate host grant required"),
        close_owned_resources=cleanup)
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        try:
            result = USAGE.exercise(harness, None, None, None, root, root, "inert-job",
                                   {"native/src/session_dashboard.html": "a" * 64})
        except RuntimeError:
            require(events[-2:] == ["browser-close", "actor-close"] if "browser-start" in events
                    else events[-1:] == ["actor-close"], "Failure leaked resources")
            raise
    return result, events


class CountsAdmission(unittest.TestCase):
    def test_real_statistics_protocol_admission_with_inert_node(self):
        result = subprocess.run(["node", "--test", str(SCRIPTS / "tests/host_tool_usage_browser_protocol.test.cjs")],
                                capture_output=True, text=True, encoding="utf-8", timeout=20)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    def test_exact_decimal_counts_are_preserved_above_javascript_precision(self):
        data = view()
        data["tools"]["read_file"]["outputTokens"] = "18446744073709551615"
        self.assertEqual(USAGE.admit_counts(require, data), data)

    def test_malformed_unselected_or_disclosing_statistics_refuse(self):
        for field, value in (("revision", "01"), ("revision", 4), ("revision", "18446744073709551616"),
                             ("hostGrantRemoteAttempts", "1"), ("estimator", "TIKTOKEN_GPT4O"),
                             ("scope", "other"), ("tools", []), ("extra", "private text")):
            data = view()
            data[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(RuntimeError):
                USAGE.admit_counts(require, data)
        for row in ({"numTimesCalled": "1", "inputTokens": "1", "outputTokens": "-1"},
                    {"numTimesCalled": "1", "inputTokens": "1", "outputTokens": "1", "text": "secret"}):
            data = view()
            data["tools"]["read_file"] = row
            with self.assertRaises(RuntimeError):
                USAGE.admit_counts(require, data)

    def test_failed_stale_or_double_counted_read_refuses(self):
        before, after = view(), view()
        after["revision"] = "5"
        after["tools"]["read_file"] = {"numTimesCalled": "3", "inputTokens": "20", "outputTokens": "30"}
        USAGE.admit_read_increment(require, before, after, 7)
        for key, value in (("numTimesCalled", "2"), ("numTimesCalled", "4"), ("inputTokens", "21"),
                           ("outputTokens", "23")):
            bad = copy.deepcopy(after)
            bad["tools"]["read_file"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                USAGE.admit_read_increment(require, before, bad, 7)
        with self.assertRaises(RuntimeError):
            USAGE.admit_read_increment(require, after, after, 7)
        with self.assertRaises(RuntimeError):
            USAGE.admit_read_increment(require, before, after, 8)

    def test_reset_must_advance_revision_and_preserve_remote_diagnostic(self):
        before = view()
        after = {**before, "revision": "5", "tools": {}}
        USAGE.admit_clear(require, before, after)
        for bad in (before, {**after, "revision": "4"}, {**after, "revision": "6"},
                    {**after, "hostGrantRemoteAttempts": "1"}):
            with self.assertRaises(RuntimeError):
                USAGE.admit_clear(require, before, bad)

    def test_history_profiles_and_guards_remain_separate(self):
        source = (SCRIPTS / "host_history_browser.py").read_text(encoding="utf-8")
        workflow = (SCRIPTS.parent / ".github/workflows/rust.yml").read_text(encoding="utf-8")
        self.assertIn('PROFILES = {**VARIANTS, "tool-usage":', source)
        self.assertIn('variant == "tool-usage"', source)
        self.assertEqual(workflow.count('--variant tool-usage --native target/release/yoke.exe'), 2)
        self.assertIn('workerAllowanceSeconds": 180', source)
        self.assertIn('digest = sha(native)  # First native payload read:', source)
        self.assertIn('validate_producer(spec["producer"]', source)

    def test_orchestration_has_real_call_order_stale_review_and_independent_natural_exits(self):
        result, events = orchestrate()
        USAGE.admit_result(require, result)
        self.assertEqual(events, ["job-admission", "actor-start", "read", "read", "browser-start",
            "before", "pending", "read", "afterdesktop", "aftermobile", "browser-natural-exit",
            "actor-natural-exit", "browser-close", "actor-close"])
        self.assertEqual(USAGE.artifact_states(result)[:4],
                         ["desktop-before", "mobile-before", "mobile-pending", "desktop-pending"])

    def test_cancel_conflict_identity_and_shutdown_failure_refuse_and_close_all_owned_resources(self):
        for fault in ("cancel", "conflict", "identity", "shutdown", "output", "output-wrapper"):
            with self.subTest(fault=fault), redirect_stderr(io.StringIO()), self.assertRaises(RuntimeError):
                orchestrate(fault)

    def test_text_only_mcp_envelope_counts_the_original_compatibility_payload(self):
        result, _ = orchestrate("text-fallback")
        USAGE.admit_result(require, result)

    def test_private_actor_error_emits_only_a_fixed_phase_and_still_closes_resources(self):
        output = io.StringIO()
        with redirect_stderr(output), self.assertRaises(RuntimeError):
            orchestrate("actor-error")
        self.assertEqual(output.getvalue(), "Owned tool statistics browser failed: seed-counts\n")

    def test_missing_semantic_or_lifecycle_evidence_never_becomes_success(self):
        result, _ = orchestrate()
        for key, value in (("automationComplete", False), ("sourceUnchanged", False),
                           ("persistedIdentityUnchanged", False), ("nativeExits", []), ("browserExit", {})):
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                USAGE.admit_result(require, {**result, key: value})
        for key, change in (("conflict", {"staleResetRefused": False}), ("cleared", {"resetRequests": 2}),
                            ("pending", {"cancelNoRequest": False})):
            bad = copy.deepcopy(result)
            bad[key].update(change)
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                USAGE.admit_result(require, bad)


if __name__ == "__main__":
    unittest.main()
