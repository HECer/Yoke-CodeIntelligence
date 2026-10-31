"""Real statistics flow, called only inside the admitted browser worker.

Importing defines inert helpers. The parent history harness retains exclusive
ownership of producer admission, image access, Job bounds and cleanup.
"""
import json
import re
import sys

VARIANT = "tool-usage"
OUTPUT = "ci-tool-usage-browser"
NODE_IDS = ("usage-estimator", "usage-calls", "usage-input", "usage-output", "usage-total",
            "usage-status", "usage-filter", "usage-sort", "clear-usage", "management-status")


def decimal(require, value):
    require(type(value) is str and re.fullmatch(r"0|[1-9][0-9]{0,19}", value) is not None
            and int(value) <= 2**64 - 1, "Exact unsigned count required")
    return int(value)


def admit_counts(require, value):
    require(type(value) is dict and set(value) == {"schema", "estimator", "scope", "revision",
            "hostGrantRemoteAttempts", "tools"}, "Closed count-only statistics required")
    require(value["schema"] == "native_tool_usage_display_v1" and value["estimator"] == "CHAR_COUNT"
            and value["scope"] == "current_session_instance_successful_serena_operations"
            and value["hostGrantRemoteAttempts"] == "0", "Owned local estimator required")
    decimal(require, value["revision"])
    tools = value["tools"]
    require(type(tools) is dict and len(tools) <= 96, "Tool count capacity")
    for name, row in tools.items():
        require(type(name) is str and re.fullmatch(r"[A-Za-z0-9_]{1,100}", name) is not None
                and type(row) is dict and set(row) == {"numTimesCalled", "inputTokens", "outputTokens"},
                "Count-only tool row required")
        for count in row.values():
            decimal(require, count)
    return value


def admit_read_increment(require, before, after, expected_output):
    admit_counts(require, before)
    admit_counts(require, after)
    require(int(after["revision"]) == int(before["revision"]) + 1, "One successful operation required")
    prior = before["tools"].get("read_file", dict.fromkeys(("numTimesCalled", "inputTokens", "outputTokens"), "0"))
    current = after["tools"].get("read_file", {})
    # This exact compact argument JSON has 27 Unicode scalars => 6 tokens.
    expected_input = len(json.dumps({"relative_path": "main.rs"}, separators=(",", ":"))) // 4
    require(set(current) == set(prior)
            and int(current["numTimesCalled"]) == int(prior["numTimesCalled"]) + 1
            and int(current["inputTokens"]) == int(prior["inputTokens"]) + expected_input
            and type(expected_output) is int and expected_output > 0
            and int(current["outputTokens"]) == int(prior["outputTokens"]) + expected_output,
            "Successful read counters differ")
    require({k: v for k, v in before["tools"].items() if k != "read_file"}
            == {k: v for k, v in after["tools"].items() if k != "read_file"}, "Other tool counters changed")


def admit_clear(require, before, after):
    admit_counts(require, before)
    admit_counts(require, after)
    require(after["tools"] == {} and int(after["revision"]) == int(before["revision"]) + 1
            and after["hostGrantRemoteAttempts"] == before["hostGrantRemoteAttempts"],
            "Explicit reset counters differ")


def identity(view):
    return {key: view[key] for key in ("activeProject", "revision")}, view["management"]["sessionStateSha256"]


def admit_result(require, result):
    require(result.get("variant") == VARIANT and result.get("owningProtocol") == "serena"
            and result.get("automationComplete") is True and result.get("sourceUnchanged") is True
            and result.get("persistedIdentityUnchanged") is True, "Statistics result incomplete")
    require(result.get("browserStart", {}).get("variant") == VARIANT, "Statistics browser selection differs")
    require(result.get("pending", {}).get("cancelNoRequest") is True
            and result.get("conflict", {}).get("staleResetRefused") is True
            and result.get("cleared", {}).get("explicitResetObserved") is True, "Reset lifecycle incomplete")
    for key in ("initial", "advanced", "final"):
        admit_counts(require, result[key])
    admit_read_increment(require, result["initial"], result["advanced"], result["advancedReadOutputTokens"])
    admit_clear(require, result["advanced"], result["final"])
    require(result["conflict"].get("resetRequests") == 1 and result["cleared"].get("resetRequests") == 1,
            "Exactly one request per explicit confirmation required")
    require(result.get("nativeExits") == [{"nativeExitCode": 0, "stdinWasStillOpen": True,
            "beforeJobCleanup": True}], "Natural owning Session exit required")
    exit_proof = result.get("browserExit", {})
    require(all(exit_proof.get(key) is True for key in ("browserCloseRequested", "cdpEOF", "browserOutputEOF",
            "driverStdoutEOF", "beforeForcedCleanup")) and exit_proof.get("browserExitCode") == 0
            and exit_proof.get("driverExitCode") == 0
            and exit_proof.get("statisticsResetRequests") == {"desktop": 1, "mobile": 1}
            and exit_proof.get("scriptErrors") == 0 and exit_proof.get("consoleErrors") == 0,
            "Natural browser/driver exits and final reset request totals required")


def artifact_states(result):
    return result["before"]["states"] + result["pending"]["states"] + [result["conflict"], result["cleared"]]


def exercise(harness, native, node, edge, fixture, output, job_name, inputs):
    require, live = harness.require, harness.LIVE
    live.require_worker_job(job_name)
    source = fixture / "source"
    source.mkdir()
    original = "// owned Unicode \u03c0 \U0001f980 e\u0301\r\nfn unchanged() {}\r\n".encode()
    target = source / "main.rs"
    target.write_bytes(original)
    profile = fixture / "statistics-host.json"
    harness.write_json(profile, {"context": "desktop-app", "modes": ["editing"], "steering": {},
        "tokenCountEstimator": "CHAR_COUNT", "dashboard": {"management": {"sessionControls": False,
        "memories": False, "toolUsageReset": True, "shutdown": True, "operationTimeoutMs": 3000},
        "lifetimeMs": 600000}})
    actors, browser = [], None
    phase = "session-start"
    try:
        actor = live.Actor(native, source, profile, output, "browser-statistics", "serena")
        actors.append(actor)
        baseline = identity(actor.state()["view"])

        def counts():
            state = actor.state()["view"]
            require(identity(state) == baseline and target.read_bytes() == original,
                    "Statistics changed persisted identity or source")
            return admit_counts(require, state["toolUsageStatisticsDisplay"])

        def read_output_tokens():
            reply = actor.rpc("tools/call", {"name": "read_file", "arguments": {"relative_path": "main.rs"}})
            require(reply.get("isError") is False, "Owning read refused")
            value = reply.get("structuredContent")
            if value is None:
                value = live.decode(reply["content"][0]["text"].encode())
            require(type(value) is dict and set(value) == {"status", "data"}
                    and value["status"] == "ok" and type(value["data"]) is dict, "Owning read incomplete")
            # The compatibility result is counted before protocol.rs adds this
            # MCP success envelope. Count that original result as Unicode scalars.
            encoded = json.dumps(value["data"], ensure_ascii=False, separators=(",", ":"), sort_keys=True, allow_nan=False)
            require(len(encoded.encode()) <= live.MAX_JSON, "Owned read result capacity")
            return len(encoded) // 4

        phase = "seed-counts"
        for _ in range(2):
            before = counts()
            expected_output = read_output_tokens()
            admit_read_increment(require, before, counts(), expected_output)
        initial = counts()
        phase = "browser-start"
        browser = harness.Browser(node, edge, fixture, output, actor, job_name,
                                  inputs["native/src/session_dashboard.html"], VARIANT)
        phase = "browser-before"
        ready = browser.call({"phase": "before", "counts": initial})
        phase = "browser-pending"
        pending = browser.call({"phase": "pending", "counts": initial})
        require(counts() == initial and pending.get("cancelNoRequest") is True
                and pending.get("expectedRevision") == initial["revision"], "Cancel/open reset changed totals")
        phase = "intervening-count"
        expected_output = read_output_tokens()
        advanced = counts()
        admit_read_increment(require, initial, advanced, expected_output)
        phase = "stale-reset"
        conflict = browser.call({"phase": "after", "viewport": "desktop", "counts": advanced})
        require(counts() == advanced and conflict.get("staleResetRefused") is True,
                "Stale reset changed counted calls")
        phase = "fresh-reset"
        cleared = browser.call({"phase": "after", "viewport": "mobile", "counts": advanced})
        final = counts()
        admit_clear(require, advanced, final)
        phase = "browser-exit"
        browser_exit = browser.shutdown()
        require(counts() == final, "Browser shutdown changed statistics")
        phase = "native-exit"
        exits = [actor.shutdown()]
        require(target.read_bytes() == original, "Source changed during shutdown")
        return {"variant": VARIANT, "owningProtocol": "serena", "automationComplete": True,
            "browserStart": browser.start, "before": ready, "pending": pending,
            "conflict": conflict, "cleared": cleared, "initial": initial, "advanced": advanced, "final": final,
            "advancedReadOutputTokens": expected_output,
            "sourceUnchanged": True, "persistedIdentityUnchanged": True,
            "browserExit": browser_exit, "nativeExits": exits}
    except Exception:
        # phase is assigned only fixed literals here; no private error text,
        # protocol, fixture path or capability enters the uploaded diagnostics.
        print("Owned tool statistics browser failed: " + phase, file=sys.stderr)
        raise
    finally:
        harness.close_owned_resources(browser, actors, [])
