"""Separate, fail-closed Windows CI real-browser host-history acceptance.

No local/offline/mock mode. The producer markers attest the existing release
build's selected inputs, without opening a native image. Only an admitted worker
in its parent's named Job may hash or launch that image. See host_history_browser.md.
"""
import argparse
import base64
import ctypes
import importlib.util
import json
import os
from pathlib import Path
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
_loader = importlib.util.spec_from_file_location("owned_history_live", ROOT / "scripts/host_history_live.py")
LIVE = importlib.util.module_from_spec(_loader)
_loader.loader.exec_module(LIVE)
_http_loader = importlib.util.spec_from_file_location("owned_history_browser_http", ROOT / "scripts/host_history_browser_http.py")
HTTP = importlib.util.module_from_spec(_http_loader)
_http_loader.loader.exec_module(HTTP)
_usage_loader = importlib.util.spec_from_file_location("owned_tool_usage_browser", ROOT / "scripts/host_tool_usage_browser.py")
USAGE = importlib.util.module_from_spec(_usage_loader)
_usage_loader.loader.exec_module(USAGE)
require, sha, read_json, write_json = LIVE.require, LIVE.sha, LIVE.read_json, LIVE.write_json
worker_job_name = LIVE.worker_job_name
MAX_JSON, MAX_OUTPUT, MIN_FREE = LIVE.MAX_JSON, LIVE.MAX_OUTPUT, LIVE.MIN_FREE
SCHEMA = "owned_history_browser_v2"
AGENT = "Code Intelligence"
VERSION = "0.5.0"
DRIVER = ROOT / "scripts/host_history_browser.cjs"
EDGES = (Path("C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"),
         Path("C:/Program Files/Microsoft/Edge/Application/msedge.exe"))
DRAFT = "\ufeff# owned browser draft π 🦀 e\u0301 İ\r\nvalue: reviewed\r\n"
VARIANTS = {"native": ("native", "ci-host-document-history-browser"),
            "serena": ("serena", "ci-host-document-history-browser-serena"),
            "persistent-python": ("serena", "ci-host-document-history-browser-persistent-python"),
            "http": ("session", "ci-host-document-history-browser-http")}
PROFILES = {**VARIANTS, "tool-usage": ("serena", USAGE.OUTPUT)}


def variant_contract(variant):
    require(type(variant) is str and variant in PROFILES, "Fixed browser variant required")
    return PROFILES[variant]


def variant_output(variant):
    return ROOT / "validation" / variant_contract(variant)[1]


def output_size(root):
    try:
        return LIVE.output_size(root)
    except Exception as error:
        name = Path(root).name
        role = ("evidence" if name in {row[1] for row in PROFILES.values()} else
                "fixture" if name.startswith("yoke-owned-history-browser-") else "other")
        reasons = {"Unexpected evidence link": "link", "Evidence entry capacity exceeded": "entries",
                   "Evidence byte capacity exceeded": "bytes"}
        reason = reasons.get(str(error), "os-error" if isinstance(error, OSError) else "other")
        print("Owned browser output scan failed: " + role + " " + reason, file=sys.stderr)
        if isinstance(error, LIVE.EvidenceLinkRefusal):
            print(f"Owned browser refused link: area={error.area}; entry={error.entry}; kind={error.kind}; depth={error.depth}",
                  file=sys.stderr)
            print("Owned browser link route: " + "/".join(error.route)
                  + "; componentSha256=" + ",".join(error.component_sha256), file=sys.stderr)
        raise


def ci_identity():
    require(os.name == "nt" and os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_OS") == "Windows", "Explicit Windows CI required")
    values = {key: os.environ.get(key, "") for key in ("GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT")}
    require(re.fullmatch(r"[0-9a-f]{40}", values["GITHUB_SHA"])
            and all(re.fullmatch(r"[1-9][0-9]{0,19}", values[key])
                    for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT")), "CI identity unavailable")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    require(revision == values["GITHUB_SHA"], "Exact CI revision required")
    subprocess.run(["git", "diff", "--no-ext-diff", "--no-textconv", "--exit-code", "HEAD", "--"],
                   cwd=ROOT, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return values


def plain_path(path, *, exists=True):
    path = Path(path)
    require(path.is_absolute() and all(not p.is_symlink() and not p.is_junction()
            for p in (path, *path.parents)), "Absolute unlinked path required")
    require(path == path.resolve(strict=exists), "Canonical path required")
    return path


def private_profile_environment(fixture):
    """Create one ordinary private Windows profile, without inherited user paths."""
    fixture = plain_path(fixture)
    require(fixture.is_dir(), "Private fixture directory required")
    home = fixture / "home"
    home.mkdir()  # A preexisting profile, including any link, must refuse.
    # Windows can create a legacy Content.IE5 junction on first cache use.
    # Occupy that exact private compatibility name with an ordinary directory;
    # the original scan still refuses any replacement or other link.
    for relative in ("AppData/Local/Temp", "AppData/LocalLow", "AppData/Roaming",
                     "AppData/Local/Microsoft/Windows/INetCache/Content.IE5",
                     "Desktop", "Documents", "Downloads"):
        path = home / relative
        path.mkdir(parents=True)
        plain_path(path)
    paths = {"HOME": home, "USERPROFILE": home, "APPDATA": home / "AppData/Roaming",
             "LOCALAPPDATA": home / "AppData/Local", "TEMP": home / "AppData/Local/Temp",
             "TMP": home / "AppData/Local/Temp"}
    return {key: str(plain_path(path)) for key, path in paths.items()}


def native_path(path):
    path = plain_path(path)
    require(path == ROOT / "target/release/yoke.exe" and path.is_file(), "Exact release producer required")
    return path


def marker_paths():
    root = plain_path(Path(os.environ.get("RUNNER_TEMP", "")))
    require(root.is_dir() and not root.is_relative_to(ROOT), "Private CI temporary root required")
    return root / "yoke-history-browser-build-start.json", root / "yoke-history-browser-build.json"


def mark_producer(phase):
    identity = ci_identity()
    start, finish = marker_paths()
    inputs = LIVE.source_inputs()
    if phase == "start":
        write_json(start, {"schema": SCHEMA, "ci": identity, "sourceInputs": inputs,
                           "startedNs": time.time_ns()})
    else:
        begun = read_json(start)
        require(begun["schema"] == SCHEMA and begun["ci"] == identity
                and begun["sourceInputs"] == inputs, "Release inputs changed during build")
        native = native_path(ROOT / "target/release/yoke.exe")  # Metadata only; never opens EXE/PDB.
        stat = native.stat()
        now = time.time_ns()
        require(0 < now - begun["startedNs"] <= 45 * 60 * 1_000_000_000
                and 0 < stat.st_size <= 160_000_000 and 0 < stat.st_mtime_ns <= now,
                "Release build metadata differs")
        write_json(finish, {**begun, "finishedNs": now, "native": str(native),
                           "nativeSize": stat.st_size, "nativeMtimeNs": stat.st_mtime_ns})


def validate_producer(value, identity, inputs, native, now):
    require(type(value) is dict and set(value) == {"schema", "ci", "sourceInputs", "startedNs",
            "finishedNs", "native", "nativeSize", "nativeMtimeNs"}, "Closed producer schema required")
    require(value["schema"] == SCHEMA and value["ci"] == identity and value["sourceInputs"] == inputs
            and value["native"] == str(native), "Exact producer identity differs")
    require(all(type(value[key]) is int for key in
                ("startedNs", "finishedNs", "nativeSize", "nativeMtimeNs")), "Producer integer fields required")
    require(0 < value["startedNs"] <= value["finishedNs"] <= now
            and now - value["finishedNs"] <= 15 * 60 * 1_000_000_000
            and value["finishedNs"] - value["startedNs"] <= 45 * 60 * 1_000_000_000
            and 0 < value["nativeSize"] <= 160_000_000
            and 0 < value["nativeMtimeNs"] <= value["finishedNs"], "Producer freshness/capacity differs")
    stat = native.stat()
    require((stat.st_size, stat.st_mtime_ns) == (value["nativeSize"], value["nativeMtimeNs"]),
            "Release metadata changed since successful build")


def preflight(phase, operation):
    """Report only the fixed admission phase; exception details may be private."""
    allowed = {"ci-identity", "arguments", "disk-reserve", "native-path", "node-path",
               "node-path-absolute", "node-path-links", "node-path-resolve",
               "node-path-resolved-links", "node-path-canonical",
               "source-inputs", "producer-marker", "producer-admission", "output-path",
               "output-create", "private-profile"}
    if phase not in allowed:
        raise ValueError("Unknown browser preflight phase")
    try:
        return operation()
    except Exception:
        print("Owned browser preflight failed: " + phase, file=sys.stderr)
        raise


def preflight_node_path(path):
    # Reject links in the selected spelling before resolving it. Windows may
    # return a different canonical spelling for the same unlinked file.
    path = Path(path)
    preflight("node-path-absolute", lambda: require(path.is_absolute(), "Absolute path required"))
    preflight("node-path-links", lambda: require(all(not p.is_symlink() and not p.is_junction()
              for p in (path, *path.parents)), "Unlinked path required"))
    def resolve_selected():
        try:
            return path.resolve(strict=True)
        except OSError as error:
            # Emit a closed classification only: exception text can contain paths.
            kind = ("missing" if isinstance(error, FileNotFoundError) else
                    "denied" if isinstance(error, PermissionError) else "other-os-error")
            print("Owned browser node resolution: " + kind, file=sys.stderr)
            selected = str(path)
            if kind == "other-os-error":
                print("Owned browser node path shape: " +
                      ("multiple-colons" if selected.count(":") > 1 else
                       "quote" if '"' in selected else "other"), file=sys.stderr)
            for label, value in (("errno", error.errno), ("winerror", getattr(error, "winerror", None))):
                if type(value) is int and 0 <= value <= 65535:
                    print(f"Owned browser node resolution {label}: {value}", file=sys.stderr)
            raise
    resolved = preflight("node-path-resolve", resolve_selected)
    preflight("node-path-resolved-links", lambda: require(resolved.is_absolute()
              and all(not p.is_symlink() and not p.is_junction() for p in (resolved, *resolved.parents)),
              "Absolute unlinked path required"))
    return preflight("node-path-canonical", lambda: plain_path(resolved))


def validate_spec(spec, spec_path):
    require(type(spec) is dict and set(spec) == {"schema", "workerJobName", "nonce", "native", "fixture",
            "output", "node", "producer", "inputs", "ci", "variant"}, "Closed worker schema required")
    require(spec["schema"] == SCHEMA and isinstance(spec["nonce"], str)
            and re.fullmatch(r"[0-9a-f]{64}", spec["nonce"])
            and spec["nonce"] == os.environ.get("YOKE_BROWSER_WORKER_NONCE"), "Owned worker nonce required")
    variant_contract(spec["variant"])
    require(spec["variant"] == os.environ.get("YOKE_BROWSER_WORKER_VARIANT"), "Owned worker variant required")
    worker_job_name(spec["workerJobName"])
    fixture, output = (plain_path(spec[key]) for key in ("fixture", "output"))
    require(spec_path == fixture / "spec.json" and fixture.name.startswith("yoke-owned-history-browser-")
            and fixture.parent == plain_path(Path(os.environ["RUNNER_TEMP"]))
            and output == variant_output(spec["variant"]), "Owned worker layout required")
    native = native_path(spec["native"])
    node = plain_path(spec["node"])
    require(node.name.lower() == "node.exe" and node.is_file() and not node.is_relative_to(ROOT)
            and not node.is_relative_to(fixture), "Explicit trusted host Node required")
    return native, fixture, output, node


def success_proof(row):
    return (type(row) is dict and row.get("assignedBeforeResume") is True
            and type(row.get("exitCode")) is int and row.get("exitCode") == 0
            and row.get("treeExitedBeforeCleanup") is True and row.get("activeBeforeCleanup") == 0
            and type(row.get("activeBeforeCleanup")) is int
            and type(row.get("activeAfterCleanup")) is int and row.get("activeAfterCleanup") == 0)


def admit_driver_reply(value, phase, capability):
    require(type(value) is dict and value.get("phase") == phase and value.get("ok") is True,
            "Real browser phase failed")
    encoded = json.dumps(value, ensure_ascii=False, allow_nan=False)
    require(len(encoded.encode()) <= MAX_JSON and capability not in encoded
            and not re.search(r"(?:https?|wss?)://|Bearer\s", encoded, re.IGNORECASE),
            "Private driver data cannot enter evidence")
    return value


def report_driver_failure(value, phase):
    if type(value) is not dict or value.get("ok") is not False:
        return
    allowed = {"phase-entry", "pending-refresh", "pending-click", "pending-receipt", "pending-admission",
               "pending-disabled", "pending-tab", "pending-repeat-click", "pending-repeat-receipt",
               "pending-keyboard-focus", "pending-repeat-admission", "pending-snapshot"}
    shape = (set(value) == {"phase", "ok", "reason", "checkpoint", "viewport"}
             and value.get("phase") == phase and value.get("reason") == "real-browser-failed")
    checkpoint = value.get("checkpoint")
    viewport = value.get("viewport")
    checkpoint = checkpoint if shape and type(checkpoint) is str and checkpoint in allowed else "other"
    viewport = viewport if shape and type(viewport) is str and viewport in {"none", "desktop", "mobile"} else "other"
    try:
        sys.stderr.write(f"Owned browser driver failed: checkpoint={checkpoint}; viewport={viewport}\n")
    except (OSError, ValueError):
        pass


def verify_artifacts(output, proof):
    """Independently admit all six uploaded images/DOM files, never empty proof."""
    result = proof["result"]
    states = (USAGE.artifact_states(result) if proof.get("variant") == "tool-usage" else
              result["before"]["states"] + result["pending"]["states"] + [item["browser"] for item in result["saves"]])
    expected = {viewport + "-" + phase + ".png" for viewport in ("desktop", "mobile")
                for phase in ("before", "pending", "after")}
    require(len(states) == 6 and {item["screenshot"] for item in states} == expected,
            "Six distinct real browser state artifacts required")
    total = 0
    for state in states:
        name = state["screenshot"]
        require(state["actualScreenshotCapture"] is True and state["humanVisualReviewComplete"] is False
                and state["noHorizontalOverflow"] is True, "Screenshot claims differ")
        image = plain_path(output / name)
        size = image.stat().st_size
        total += size
        require(33 <= size == state["screenshotBytes"] <= 2_000_000 and total <= 12_000_000,
                "Screenshot byte capacity differs")
        with image.open("rb") as source:
            header = source.read(33)
        width, height = int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")
        require(header[:8] == b"\x89PNG\r\n\x1a\n" and header[12:16] == b"IHDR"
                and width == (1440 if name.startswith("desktop-") else 390)
                and width == state["width"] and 0 < height == state["height"] <= 4096
                and width * height <= 4_000_000 and sha(image) == state["screenshotSha256"],
                "Screenshot header/hash differs")
        require(state["dom"] == name[:-4] + ".dom.json", "DOM artifact name differs")
        dom_path = plain_path(output / state["dom"])
        dom = read_json(dom_path, 128000)
        if proof.get("variant") == "tool-usage":
            require(type(dom.get("nodes")) is list and [node.get("id") for node in dom["nodes"]] == list(USAGE.NODE_IDS),
                    "Selected statistics DOM nodes differ")
        require(sha(dom_path) == state["domSha256"] and dom["width"] == width
                and dom["scrollWidth"] <= width and len(dom["nodes"]) == 10
                and not re.search(r"(?:https?|wss?)://|Bearer\s", json.dumps(dom), re.IGNORECASE),
                "Selected DOM evidence differs or contains private endpoints")


class Browser:
    """Private bounded control channel; never transcribes URLs or raw CDP."""
    def __init__(self, node, browser, fixture, output, actor, job_name, dashboard_hash, variant="native"):
        LIVE.require_worker_job(job_name)
        variant_contract(variant)
        self.frames = queue.Queue(maxsize=4)
        self.error = None
        self.closed = False
        self.capability = actor.capability
        self.stderr = (fixture / "driver.stderr").open("xb")
        self.process = subprocess.Popen([str(node), "--max-old-space-size=128", str(DRIVER)], cwd=fixture,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
            creationflags=subprocess.CREATE_NO_WINDOW)
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        try:
            self.start = self.call({"phase": "start", "browser": str(browser), "profile": str(fixture / "edge-profile"),
                "output": str(output), "url": "http://127.0.0.1:" + str(actor.port) + "/#" + actor.capability,
                "document": "new", "draft": DRAFT, "dashboardSha256": dashboard_hash, "variant": variant})
            require(self.start.get("variant") == variant, "Selected driver variant receipt differs")
        except BaseException:
            self.close()
            raise

    def _read(self):
        try:
            while True:
                line = self.process.stdout.readline(MAX_JSON + 1)
                if not line:
                    return
                require(len(line) <= MAX_JSON and line.endswith(b"\n"), "Driver frame capacity")
                self.frames.put(LIVE.decode(line), timeout=2)
        except BaseException:
            self.error = "driver-reader-failed"

    def call(self, request):
        data = json.dumps(request, ensure_ascii=False).encode() + b"\n"
        require(len(data) <= 64000 and self.error is None, "Private driver input capacity")
        self.process.stdin.write(data)
        self.process.stdin.flush()
        value = self.frames.get(timeout=35)
        require(self.error is None, "Private driver reader failed")
        report_driver_failure(value, request["phase"])
        return admit_driver_reply(value, request["phase"], self.capability)

    def shutdown(self):
        proof = self.call({"phase": "close"})
        self.process.stdin.close()  # Driver must observe EOF and close its output.
        self.process.wait(timeout=8)
        self.reader.join(timeout=2)
        require(proof.get("browserCloseRequested") is True and proof.get("browserExitCode") == 0
                and proof.get("cdpEOF") is True and proof.get("browserOutputEOF") is True
                and self.process.returncode == 0
                and not self.reader.is_alive() and self.error is None and self.frames.empty(),
                "Browser/Node raw exit and EOF not proved")
        self.closed = True
        self.process.stdout.close()
        self.stderr.close()
        return {**proof, "driverExitCode": 0, "driverStdoutEOF": True, "beforeForcedCleanup": True}

    def close(self):
        if self.closed:
            return
        if self.process.poll() is None:
            self.process.kill()  # Failure cleanup only; cannot create an acceptance result.
            self.process.wait(timeout=5)
        self.reader.join(timeout=2)
        for stream in (self.process.stdin, self.process.stdout, self.stderr):
            stream.close()
        self.closed = True


def report_exercise_failure(phase):
    # Labels only: exceptions, CLI bodies and driver frames can contain private
    # capabilities or endpoints. An unavailable diagnostic sink cannot replace
    # the original failure.
    allowed = {"old-session", "seed-history", "migration", "current-session", "browser-start",
               "browser-before", "before-integrity", "history-plan", "history-selection",
               "pending-archive", "pending-frame", "browser-pending", "pending-authorities",
               "pending-integrity", "history-resume", "archive-read", "save-desktop", "save-mobile",
               "browser-shutdown", "native-shutdown", "final-integrity", "profile-admission",
               "runtime-start", "runtime-retained", "runtime-exit", "http-isolation",
               "http-session-shutdown", "http-service-exit"}
    label = phase if type(phase) is str and phase in allowed else "other"
    try:
        print("Owned browser exercise failed: " + label, file=sys.stderr)
    except (OSError, ValueError):
        pass


def trusted_runtime_python(fixture):
    # This is the already selected CI worker interpreter, never analyzed input.
    path = plain_path(Path(sys.executable))
    require(path.is_file() and path.name.lower() == "python.exe"
            and not path.is_relative_to(ROOT) and not path.is_relative_to(fixture),
            "Explicit trusted host Python required")
    return path


def owning_actor(native, source, profile, output, label, variant, python, job_name=None):
    protocol, _ = variant_contract(variant)
    require((python is not None) == (variant == "persistent-python"), "Independent Python selection required")
    if variant == "http":
        require(type(job_name) is str, "Specific HTTP worker Job required")
        return HTTP.HttpActor(native, source, profile, output, label, job_name)
    if variant == "native":
        return LIVE.Actor(native, source, profile, output, label)
    return LIVE.Actor(native, source, profile, output, label, protocol, python)


def profile_observation(actor, variant, role):
    protocol, _ = variant_contract(variant)
    require(role in ("old", "current") and actor.protocol == protocol, "Owning protocol differs")
    catalog = actor.rpc("tools/list", {})
    tools = catalog.get("tools")
    require(type(tools) is list and 0 < len(tools) <= 512, "Bounded owning tool catalog required")
    names = [tool.get("name") for tool in tools if type(tool) is dict]
    require(len(names) == len(tools) and all(type(name) is str and re.fullmatch(r"[a-z][a-z0-9_]{0,95}", name)
            for name in names) and len(set(names)) == len(names), "Canonical owning tool names required")
    if variant == "http":
        require("ci_session" in names and not {"get_current_config", "open_dashboard", "serena_repl"}.intersection(names),
                "Selected HTTP Session tool contract differs")
        return {"role": role, "protocol": protocol, "toolCount": len(names),
                "sessionDashboardToolObserved": True, "serenaDashboardToolsAbsent": True}
    require({"get_current_config", "open_dashboard"}.issubset(names) and "ci_session" not in names
            and (variant != "persistent-python" or "serena_repl" in names), "Selected Serena tool contract differs")
    return {"role": role, "protocol": protocol, "toolCount": len(names),
            "serenaDashboardToolsObserved": True, "nativeSessionToolAbsent": True,
            "persistentToolObserved": "serena_repl" in names}


class PersistentRuntime:
    def __init__(self, actor, python, role):
        require(role in ("old", "current"), "Fixed runtime role required")
        self.actor, self.role = actor, role
        self.retained = False
        self.session = actor.tool("get_current_config")["sessionId"]
        require(type(self.session) is str and 0 < len(self.session) <= 128, "Owned Session identifier required")
        require(self.call("owned_marker = 45\nprint(owned_marker)") == "45\n", "Persistent initialization differs")
        pid = self.call("import os\nprint(os.getpid())").strip()
        require(re.fullmatch(r"[1-9][0-9]{0,9}", pid), "Bounded owned runtime process required")
        self.handle = LIVE.RuntimeHandle(int(pid), python)

    def call(self, code):
        return self.actor.tool("serena_repl", {"session_id": self.session, "code": code})["stdout"]

    def require_retained(self):
        self.retained = False
        require(self.call("print(owned_marker)") == "45\n", "Persistent runtime value changed")
        self.retained = True

    def require_exited(self):
        require(self.retained, "Retained runtime value must precede exit proof")
        self.handle.require_exited()
        return {"role": self.role, "marker45Retained": True, "imageIdentityVerified": True,
                "exitBeforeJobCleanup": True}

    def close(self):
        self.handle.close()


def close_owned_resources(browser, actors, runtimes):
    # Attempt every owned teardown even when one cleanup refuses. No cleanup
    # exception can manufacture a successful automation receipt.
    failure = None
    for resource in ([browser] if browser else []) + list(reversed(actors)) + list(reversed(runtimes)):
        try:
            resource.close()
        except BaseException as error:
            if failure is None:
                failure = error
    if failure is not None:
        raise failure


def admit_variant_result(result, variant):
    if variant == "tool-usage":
        return USAGE.admit_result(require, result)
    protocol, _ = variant_contract(variant)
    require(result.get("variant") == variant and result.get("owningProtocol") == protocol
            and result.get("automationComplete") is True, "Selected browser result differs")
    require(type(result.get("browserStart")) is dict and result["browserStart"].get("variant") == variant,
            "Selected browser startup receipt required")
    rows = result.get("profileObservations")
    require(type(rows) is list, "Owning profile observations required")
    if variant == "native":
        require(rows == [], "Native default observations differ")
    elif variant == "http":
        require(len(rows) == 2 and all(type(row) is dict for row in rows)
                and [row.get("role") for row in rows] == ["old", "current"]
                and all(row.get("protocol") == "session" and row.get("sessionDashboardToolObserved") is True
                        and row.get("serenaDashboardToolsAbsent") is True and type(row.get("toolCount")) is int
                        and 0 < row["toolCount"] <= 512 for row in rows), "Both owning HTTP profiles required")
    else:
        require(len(rows) == 2 and all(type(row) is dict for row in rows)
                and [row.get("role") for row in rows] == ["old", "current"]
                and all(row.get("protocol") == protocol and row.get("serenaDashboardToolsObserved") is True
                        and row.get("nativeSessionToolAbsent") is True and type(row.get("toolCount")) is int
                        and 0 < row["toolCount"] <= 512
                        and (variant != "persistent-python" or row.get("persistentToolObserved") is True)
                        for row in rows), "Both owning Serena profiles required")
    runtimes = result.get("persistentRuntimes")
    if variant == "persistent-python":
        require(type(runtimes) is list and len(runtimes) == 2
                and all(type(row) is dict for row in runtimes)
                and [row.get("role") for row in runtimes] == ["current", "old"]
                and all(row.get("marker45Retained") is True and row.get("imageIdentityVerified") is True
                        and row.get("exitBeforeJobCleanup") is True for row in runtimes),
                "Both retained runtime values and independent exits required")
    else:
        require(runtimes == [], "Unselected runtime claim refused")
    if variant == "http":
        require(result.get("owningTransport") == "http" and result.get("nativeExits") == [],
                "Distinct HTTP service/Session proof required")
        authority = result.get("httpAuthorities")
        require(type(authority) is list and len(authority) == 2 and all(type(row) is dict for row in authority)
                and [row.get("role") for row in authority] == ["old", "current"]
                and all(row.get("foreignSessionRefused") is True and row.get("authenticationRefusals") == [401, 401, 403]
                        for row in authority), "Both HTTP authorization refusals required")
        sessions, services = result.get("httpSessionExits"), result.get("httpServiceExits")
        require(type(sessions) is list and len(sessions) == 2 and all(type(row) is dict for row in sessions)
                and [row.get("role") for row in sessions] == ["current", "old"]
                and all(row.get("scope") == "owning_http_mcp_session" and row.get("revokedSessionStatus") == 404
                        and row.get("serviceStillRunning") is True and row.get("beforeJobCleanup") is True
                        for row in sessions), "HTTP Session isolation/closure required")
        require(type(services) is list and len(services) == 2 and all(type(row) is dict for row in services)
                and [row.get("role") for row in services] == ["current", "old"]
                and all(type(row.get("nativeExitCode")) is int and row["nativeExitCode"] == 0
                        and row.get("stdinWasStillOpen") is True and row.get("stdoutEOF") is True
                        and row.get("naturalHostDurationMs") == HTTP.SERVICE_MS
                        and type(row.get("observedElapsedMs")) is int
                        and HTTP.SERVICE_MS <= row["observedElapsedMs"] <= HTTP.SERVICE_MS + 18000
                        and row.get("beforeJobCleanup") is True and row.get("logonScopedCredential") is True
                        and row.get("ownedCredentialRemoved") is True for row in services),
                "Both natural HTTP service exits and credential cleanup required")
        require(result.get("peerHttpSessionSurvived") is True, "Independent HTTP Session survival required")
    else:
        require(all(result.get(key, []) == [] for key in ("httpAuthorities", "httpSessionExits", "httpServiceExits")),
                "Unselected HTTP claim refused")


def exercise(native, node, edge, fixture, output, job_name, inputs, variant="native"):
    if variant == "tool-usage":
        return USAGE.exercise(SimpleNamespace(require=require, LIVE=LIVE, write_json=write_json,
                             Browser=Browser, close_owned_resources=close_owned_resources),
                             native, node, edge, fixture, output, job_name, inputs)
    LIVE.require_worker_job(job_name)
    protocol, _ = variant_contract(variant)
    python = trusted_runtime_python(fixture) if variant == "persistent-python" else None
    source, old, new, archive, host = [fixture / name for name in ("source-İ", "old", "new", "archive-π", "host")]
    for path in (source, old, new, archive, host):
        path.mkdir()
    source_bytes = "// owned inert e\u0301 🦀\r\nfn untouched() {}\r\n".encode()
    (source / "main.rs").write_bytes(source_bytes)
    target = host / "settings-π.yaml"
    original = "\ufeff# owned global π\r\nvalue: preserved\r\n".encode()
    target.write_bytes(original)
    config = {"documents": [{"id": "old", "label": "Owned old settings", "path": str(target),
               "scope": {"kind": "global"}, "writable": True}], "journalRoot": str(old), "maxDocumentBytes": 64000}
    destination = LIVE.decode(json.dumps(config).encode())
    destination["documents"][0].update(id="new", label="Owned destination settings")
    destination["journalRoot"] = str(new)
    roots = [{"name": "default", "root": str(source)}]
    before, after = fixture / "from.json", fixture / "to.json"
    write_json(before, {"documents": config, "projects": roots})
    write_json(after, {"documents": destination, "projects": roots})
    for label, selection in (("old", config), ("new", destination)):
        write_json(fixture / (label + "-host.json"), {"context": "desktop-app", "modes": ["editing"], "steering": {},
            "dashboard": {"management": {"sessionControls": False, "memories": False, "documents": selection,
            "shutdown": True, "operationTimeoutMs": 3000}, "lifetimeMs": 600000}})
    command = [str(native), "--timeout", "30000", "--max-bytes", "1000000", "host-documents",
        "--from-selection", str(before), "--to-selection", str(after), "--history-record-budget", "32"]
    cli_rows = []

    def cli(label, arguments, success=True):
        # CLI output remains private, including any future diagnostic capabilities.
        stdout, stderr = fixture / (label + ".stdout"), fixture / (label + ".stderr")
        with stdout.open("xb") as out, stderr.open("xb") as err:
            result = subprocess.run(command + arguments, cwd=source, stdout=out, stderr=err, timeout=20,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
        require(stdout.stat().st_size <= MAX_JSON and stderr.stat().st_size <= MAX_JSON, "CLI log capacity")
        require((result.returncode == 0) == success, "Original host CLI outcome differs")
        cli_rows.append({"operation": label, "exitCode": result.returncode, "expectedSuccess": success})
        return LIVE.decode(stdout.read_bytes()) if success else None

    def history(operation, plan, *extra):
        return [operation, "--plan", str(plan), "--archive-root", str(archive), *extra]

    actors, browser, runtimes, profiles = [], None, [], []
    phase = "old-session"
    try:
        previous = owning_actor(native, source, fixture / "old-host.json", output, "browser-old", variant, python, job_name)
        actors.append(previous)
        if variant != "native":
            phase = "profile-admission"
            profiles.append(profile_observation(previous, variant, "old"))
        if python:
            phase = "runtime-start"
            runtimes.append(PersistentRuntime(previous, python, "old"))
        phase = "seed-history"
        for saved in range(2):
            text = original.decode() + "# original saved history " + str(saved) + " 🦀\r\n"
            previous.apply("old", previous.plan("old", text))
            require(target.read_bytes() == text.encode(), "Seeded history target differs")
        old_cached = previous.plan("old", "# retired cached draft\r\n")
        phase = "migration"
        migration = cli("migration-plan", ["binding-plan", "--from-document", "old", "--to-document", "new"])
        migration_path = fixture / "migration.json"
        write_json(migration_path, migration)
        receipt = cli("migration-apply", ["binding-apply", "--plan", str(migration_path), "--allow-host-writes"])
        require(receipt["outcome"] == "committed" and receipt["fileChanged"] is False, "Migration incomplete")
        owner = host / (".yoke-doc-owner-" + migration["target"] + ".json")
        owner_hash, before_bytes = sha(owner), target.read_bytes()
        require(before_bytes == text.encode(), "Migration changed target bytes")
        phase = "current-session"
        current = owning_actor(native, source, fixture / "new-host.json", output, "browser-current", variant, python, job_name)
        actors.append(current)
        if variant != "native":
            phase = "profile-admission"
            profiles.append(profile_observation(current, variant, "current"))
        if python:
            phase = "runtime-start"
            runtimes.append(PersistentRuntime(current, python, "current"))
        http_authorities = []
        if variant == "http":
            phase = "http-isolation"
            http_authorities = [previous.require_peer_isolated(current), current.require_peer_isolated(previous)]
        phase = "browser-start"
        browser = Browser(node, edge, fixture, output, current, job_name, inputs["native/src/session_dashboard.html"], variant)
        baseline = LIVE.tree_hashes(old, new, archive, host)
        phase = "browser-before"
        ready = browser.call({"phase": "before"})
        phase = "before-integrity"
        require(LIVE.tree_hashes(old, new, archive, host) == baseline
                and target.read_bytes() == before_bytes and sha(owner) == owner_hash,
                "Browser draft/review changed owned file state")
        current_cached = {"id": ready["lastPlanId"]}
        originals = {path.name: path.read_bytes() for path in old.iterdir()
                     if path.name.startswith("yoke-doc-" + migration["fromAccess"] + "-")}
        phase = "history-plan"
        plan = cli("history-plan", ["history-plan", "--migration", str(migration_path), "--archive-root", str(archive)])
        phase = "history-selection"
        require(set(row["name"] for row in plan["records"]) == set(originals)
                and len(originals) > 0 and plan["unselectedClosedJournalsAtReview"] == 0, "Original package differs")
        plan_path = fixture / "history.json"
        write_json(plan_path, plan)
        phase = "pending-archive"
        occupant = host / (".yoke-doc-history-" + plan["id"] + ".complete.json")
        occupant.write_bytes(b"owned foreign completion")
        cli("blocked-completion", history("history-apply", plan_path, "--allow-host-writes"), False)
        pending = cli("pending-inspect", history("history-inspect", plan_path))
        require(pending["phase"] == "committed" and pending["markerPresent"] is True
                and pending["complete"] is False and pending["immutableArchiveVerified"] is True, "Real pending condition absent")
        phase = "pending-frame"
        frame = archive / ("yoke-doc-history-" + plan["id"] + ".bin")
        require(frame.stat().st_size <= LIVE.MAX_FRAME, "Frame capacity")
        rows = LIVE.admitted_frame(frame.read_bytes(), plan, originals)
        frame_hash = sha(frame)
        unchanged = LIVE.tree_hashes(old, new, archive, host)
        phase = "browser-pending"
        pending_ui = browser.call({"phase": "pending"})
        phase = "pending-authorities"
        for actor, document, cached in ((previous, "old", old_cached), (current, "new", current_cached)):
            observed = actor.document({"kind": "pending", "documentId": document})
            require(observed["complete"] is False and observed["blockedByHistoryArchive"] is True
                    and observed["journals"] == [], "Independent server blocker differs")
            actor.apply(document, cached, 422)
            actor.document({"kind": "recover", "documentId": document, "journalId": "a" * 64,
                            "expectedEvent": "b" * 64, "direction": "resume"}, 422)
        phase = "pending-integrity"
        require(unchanged == LIVE.tree_hashes(old, new, archive, host)
                and target.read_bytes() == before_bytes and sha(owner) == owner_hash, "Blocked UI/server mutated owned state")
        phase = "history-resume"
        LIVE.retain_foreign(occupant, host / "retained-foreign-completion.json", b"owned foreign completion")
        resumed = cli("history-resume", history("history-recover", plan_path, "--expected-event", pending["eventId"],
                         "--direction", "resume", "--allow-host-writes"))
        require(resumed["complete"] is True and resumed["markerPresent"] is False, "Original host resume incomplete")
        phase = "archive-read"
        for index, record in enumerate(plan["records"]):
            read = cli("read-" + str(index), history("history-read", plan_path, "--record", record["name"]))
            require(base64.b64decode(read["bytesBase64"], validate=True) == originals[record["name"]]
                    and read["sha256"] == record["hash"] and read["immutableRecordEvidenceOnly"] is True, "Exact archive read differs")
        saves = []
        for viewport in ("desktop", "mobile"):
            phase = "save-desktop" if viewport == "desktop" else "save-mobile"
            fresh = DRAFT + "# fresh explicit " + viewport + " save after host resume\r\n"
            result = browser.call({"phase": "after", "viewport": viewport, "draft": fresh})
            header = read_json(new / ("yoke-doc-" + migration["toAccess"] + "-" + result["planId"] + ".plan.json"), 512000)
            require(target.read_bytes() == fresh.encode() and sha(owner) == owner_hash and sha(frame) == frame_hash
                    and header["plan"]["ownerGeneration"] == migration["nonce"]
                    and header["plan"]["afterHash"] == sha(target), "UI guarded save bytes/generation differ")
            saves.append({"viewport": viewport, "targetSha256": sha(target), "ownerSha256": owner_hash,
                          "planId": result["planId"], "ownerGeneration": migration["nonce"], "browser": result})
        phase = "browser-shutdown"
        browser_exit = browser.shutdown()
        runtime_exits = []
        if runtimes:
            phase = "runtime-retained"
            for runtime in runtimes:
                runtime.require_retained()
        phase = "native-shutdown"
        http_sessions, http_services, peer_http_survived = [], [], False
        if variant == "http":
            phase = "http-session-shutdown"
            http_sessions.append(current.shutdown())
            require(previous.state()["view"]["management"]["sessionStateSha256"] == previous.fingerprint,
                    "Ending HTTP destination ended another Session")
            profile_observation(previous, variant, "old")
            peer_http_survived = True
            http_sessions.append(previous.shutdown())
            phase = "http-service-exit"
            http_services = [actor.require_service_exit() for actor in reversed(actors)]
            exits = []
        elif runtimes:
            exits = [current.shutdown()]
            phase = "runtime-exit"
            runtime_exits.append(runtimes[1].require_exited())
            require(previous.state()["view"]["management"]["sessionStateSha256"] == previous.fingerprint,
                    "Ending destination ended another Session")
            phase = "runtime-retained"
            runtimes[0].require_retained()
            phase = "native-shutdown"
            exits.append(previous.shutdown())
            phase = "runtime-exit"
            runtime_exits.append(runtimes[0].require_exited())
        else:
            exits = [actor.shutdown() for actor in reversed(actors)]
        phase = "final-integrity"
        require((source / "main.rs").read_bytes() == source_bytes
                and sha(frame) == frame_hash and sha(owner) == owner_hash, "Final immutable evidence differs")
        return {"automationComplete": True,
                "scope": "one selected owning MCP Session dashboard; original host CLI",
                "variant": variant, "owningProtocol": protocol, "profileObservations": profiles,
                "owningTransport": "http" if variant == "http" else "stdio",
                "persistentRuntimes": runtime_exits,
                "httpAuthorities": http_authorities, "httpSessionExits": http_sessions,
                "httpServiceExits": http_services, "peerHttpSessionSurvived": peer_http_survived,
                "browserStart": browser.start, "before": ready, "pending": pending_ui, "saves": saves,
                "archive": {"sha256": frame_hash, "records": rows, "allOriginalsReadBack": True},
                "blockedBothAuthorities": True, "blockedActionsNoMutation": True, "sourceUnchanged": True,
                "browserExit": browser_exit, "nativeExits": exits, "cli": cli_rows}
    except Exception:
        report_exercise_failure(phase)
        raise
    finally:
        close_owned_resources(browser, actors, runtimes)


def bounded_worker(spec_path, fixture, output, environment, job_name):
    # The layout and suspended-assignment ordering follow the already exercised
    # repository Windows runner, with explicit 64-bit admission and cleanup proof.
    import _winapi
    import msvcrt

    class Basic(ctypes.Structure):
        _fields_ = [("processTime", ctypes.c_int64), ("jobTime", ctypes.c_int64), ("flags", ctypes.c_uint32),
                    ("minWorking", ctypes.c_size_t), ("maxWorking", ctypes.c_size_t),
                    ("activeLimit", ctypes.c_uint32), ("affinity", ctypes.c_size_t),
                    ("priority", ctypes.c_uint32), ("scheduling", ctypes.c_uint32)]

    class Extended(ctypes.Structure):
        _fields_ = [("basic", Basic), ("io", ctypes.c_uint64 * 6), ("processMemory", ctypes.c_size_t),
                    ("jobMemory", ctypes.c_size_t), ("peakProcessMemory", ctypes.c_size_t),
                    ("peakJobMemory", ctypes.c_size_t)]

    require(ctypes.sizeof(Extended) == 144, "Unsupported Windows Job layout")
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    for name, args, result in (
        ("CreateJobObjectW", [ctypes.c_void_p, ctypes.c_wchar_p], ctypes.c_void_p),
        ("SetInformationJobObject", [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32], ctypes.c_int),
        ("AssignProcessToJobObject", [ctypes.c_void_p, ctypes.c_void_p], ctypes.c_int),
        ("QueryInformationJobObject", [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_void_p], ctypes.c_int),
        ("TerminateJobObject", [ctypes.c_void_p, ctypes.c_uint32], ctypes.c_int),
        ("ResumeThread", [ctypes.c_void_p], ctypes.c_uint32), ("CloseHandle", [ctypes.c_void_p], ctypes.c_int),
    ):
        function = getattr(api, name)
        function.argtypes, function.restype = args, result
    require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Native live acceptance disk reserve unavailable")
    job_name = worker_job_name(job_name)
    ctypes.set_last_error(0)
    job = api.CreateJobObjectW(None, job_name)
    require(bool(job), "Windows Job unavailable")
    if ctypes.get_last_error() == 183:
        require(api.CloseHandle(job), "Occupied Job handle close failed")
        raise RuntimeError("Owned worker Job name is already occupied")
    process = thread = None
    row = {"startedSuspended": True, "assignedBeforeResume": False, "complete": False}

    def active():
        info = ctypes.create_string_buffer(48)
        require(api.QueryInformationJobObject(job, 1, info, 48, None), "Windows Job query failed")
        return int.from_bytes(info.raw[40:44], "little")

    try:
        limits = Extended()
        limits.basic.flags = 0x2000
        require(api.SetInformationJobObject(job, 9, ctypes.byref(limits), ctypes.sizeof(limits)), "Job limit failed")
        with open(os.devnull, "rb") as stdin, (output / "worker.stdout").open("xb") as stdout, (output / "worker.stderr").open("xb") as stderr:
            handles = [msvcrt.get_osfhandle(stream.fileno()) for stream in (stdin, stdout, stderr)]
            for handle in handles:
                os.set_handle_inheritable(handle, True)
            startup = subprocess.STARTUPINFO()
            startup.dwFlags = subprocess.STARTF_USESTDHANDLES
            startup.hStdInput, startup.hStdOutput, startup.hStdError = handles
            startup.lpAttributeList = {"handle_list": handles}
            argv = [sys.executable, "-I", str(Path(__file__).resolve()), "--worker", str(spec_path)]
            process, thread, pid, _ = _winapi.CreateProcess(
                None, subprocess.list2cmdline(argv), None, None, True,
                subprocess.CREATE_NO_WINDOW | 0x00000004, environment, str(fixture), startup)
            row["pid"] = pid
            require(api.AssignProcessToJobObject(job, process), "Suspended Job assignment failed")
            row["assignedBeforeResume"] = True
            require(api.ResumeThread(thread) != 0xffffffff, "Worker resume failed")
            deadline = time.monotonic() + 180
            while _winapi.WaitForSingleObject(process, 100) == 258:
                require(time.monotonic() < deadline, "Owned live acceptance deadline")
                require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Owned disk-headroom stop")
                output_size(output)
                output_size(fixture)
            row["exitCode"] = _winapi.GetExitCodeProcess(process)
            row["activeAtParentExit"] = active()
            # A signalled root alone does not prove that its assigned tree has
            # exited. Observe accounting settling before forced cleanup, using
            # at most one second within the original worker deadline.
            drain_deadline = min(deadline, time.monotonic() + 1)
            remaining_active = row["activeAtParentExit"]
            while remaining_active and time.monotonic() < drain_deadline:
                time.sleep(.01)
                remaining_active = active()
            row["activeBeforeCleanup"] = remaining_active
            row["treeExitedBeforeCleanup"] = remaining_active == 0
    finally:
        if process is not None and not row["assignedBeforeResume"]:
            _winapi.TerminateProcess(process, 124)
        require(api.TerminateJobObject(job, 125), "Owned Job termination failed")
        deadline = time.monotonic() + 10
        while active() and time.monotonic() < deadline:
            time.sleep(.05)
        row["activeAfterCleanup"] = active()
        for handle in (thread, process, job):
            if handle is not None:
                require(api.CloseHandle(handle), "Owned Job handle close failed")
        row["complete"] = (row.get("exitCode") == 0
                           and row.get("treeExitedBeforeCleanup") is True
                           and row["activeAfterCleanup"] == 0)
        write_json(output / "job.json", row)
    require(row["complete"], "Owned live worker did not complete")


def worker(spec_path):
    identity = ci_identity()  # Refuses non-CI before any native image access.
    spec_path = plain_path(spec_path)
    spec = read_json(spec_path)  # Includes the bounded source map and producer map.
    native, fixture, output, node = validate_spec(spec, spec_path)
    admission = LIVE.require_worker_job(spec["workerJobName"])
    inputs = LIVE.source_inputs()
    require(spec["ci"] == identity and spec["inputs"] == inputs, "Worker source identity differs")
    validate_producer(spec["producer"], identity, inputs, native, time.time_ns())
    require(shutil.disk_usage(ROOT).free >= MIN_FREE, "Original disk reserve required")
    digest = sha(native)  # First native payload read: producer + exact named Job already admitted.
    edge = next((plain_path(path) for path in EDGES if path.is_file()), None)
    require(edge is not None, "Installed system Microsoft Edge required; no download/fallback")
    result = exercise(native, node, edge, fixture, output, spec["workerJobName"], inputs, spec["variant"])
    admit_variant_result(result, spec["variant"])
    require(sha(native) == digest and LIVE.source_inputs() == inputs, "Producer or selected inputs changed")
    write_json(output / "automation.json", {"schema": SCHEMA, "product": AGENT, "version": VERSION,
        "variant": spec["variant"],
        "nativeSha256": digest, "workerJobAdmission": admission, "result": result,
        "producer": {key: spec["producer"][key] for key in
                     ("ci", "startedNs", "finishedNs", "nativeSize", "nativeMtimeNs")},
        "realInstalledHeadlessBrowser": True, "visibleDesktopShellInteraction": False,
        "visualReviewComplete": False, "humanAcceptanceClaimed": False,
        "allTransportsClaimed": False, "releaseAcceptanceClaimed": False})
    return 0


def main(arguments=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mark-build", choices=("start", "complete"))
    parser.add_argument("--worker", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--native", type=Path)
    parser.add_argument("--node", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--variant", choices=tuple(PROFILES), default="native")
    options = parser.parse_args(arguments)
    if options.mark_build:
        require(options.variant == "native", "Build marker does not select a browser variant")
        return mark_producer(options.mark_build)
    if options.worker:
        return worker(options.worker)
    identity = preflight("ci-identity", ci_identity)
    preflight("arguments", lambda: require(options.native and options.node and options.output,
                                             "Explicit native, host Node and output required"))
    preflight("disk-reserve", lambda: require(shutil.disk_usage(ROOT).free >= MIN_FREE,
                                                "Original disk reserve required"))
    native = preflight("native-path", lambda: native_path(options.native.absolute()))
    node = preflight("node-path", lambda: preflight_node_path(options.node.absolute()))
    inputs = preflight("source-inputs", LIVE.source_inputs)
    producer = preflight("producer-marker", lambda: read_json(marker_paths()[1]))
    preflight("producer-admission", lambda: validate_producer(producer, identity, inputs, native, time.time_ns()))
    def admit_output():
        path = plain_path(options.output.absolute(), exists=False)
        require(path == variant_output(options.variant), "Exact browser evidence destination required")
        return path
    output = preflight("output-path", admit_output)
    preflight("output-create", lambda: output.mkdir(parents=True, exist_ok=False))
    fixture = Path(tempfile.mkdtemp(prefix="yoke-owned-history-browser-", dir=marker_paths()[0].parent)).resolve()
    environment = {key: os.environ[key] for key in ("PATH", "SystemRoot", "SystemDrive", "WINDIR",
        "GITHUB_ACTIONS", "RUNNER_OS", "RUNNER_TEMP", "GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT")}
    environment.update(preflight("private-profile", lambda: private_profile_environment(fixture)))
    environment.update(PYTHONUTF8="1", PYTHONDONTWRITEBYTECODE="1", DO_NOT_TRACK="1",
                       YOKE_BROWSER_WORKER_NONCE=os.urandom(32).hex(), YOKE_BROWSER_WORKER_VARIANT=options.variant)
    job_name = "Local\\YokeHistoryCI-" + os.urandom(32).hex()
    spec_path = fixture / "spec.json"
    write_json(spec_path, {"schema": SCHEMA, "workerJobName": job_name,
        "variant": options.variant,
        "nonce": environment["YOKE_BROWSER_WORKER_NONCE"], "native": str(native), "fixture": str(fixture),
        "output": str(output), "node": str(node), "producer": producer, "inputs": inputs, "ci": identity})
    write_json(output / "source-inputs.json", {"sourceRevision": identity["GITHUB_SHA"], "sha256": inputs,
        "scope": "Same selected tracked native/src/python/scripts and Cargo/package inputs as the original gate"})
    complete = False
    try:
        bounded_worker(spec_path, fixture, output, environment, job_name)
        proof = read_json(output / "automation.json")
        require(proof["schema"] == SCHEMA and proof["variant"] == options.variant, "Owned browser proof variant differs")
        admit_variant_result(proof["result"], options.variant)
        require(proof["result"]["automationComplete"] is True and success_proof(read_json(output / "job.json"))
                and LIVE.source_inputs() == inputs, "Browser automation proof incomplete")
        verify_artifacts(output, proof)
        ci_identity()
        output_size(output)
        complete = True
    finally:
        write_json(output / "result.json", {"product": AGENT, "version": VERSION, "automationComplete": complete,
            "variant": options.variant, "owningProtocol": variant_contract(options.variant)[0],
            "sourceRevision": identity["GITHUB_SHA"], "validatorSha256": sha(Path(__file__)),
            "driverSha256": sha(DRIVER), "workerAllowanceSeconds": 180, "diskStopBytes": MIN_FREE,
            "privateAndEvidenceTreeLimitBytesEach": MAX_OUTPUT, "visualReviewComplete": False,
            "humanAcceptanceClaimed": False, "visibleDesktopShellInteraction": False,
            "serenaBrowserClaimed": complete and options.variant == "serena",
            "persistentPythonBrowserClaimed": complete and options.variant == "persistent-python",
            "httpTransportBrowserClaimed": complete and options.variant == "http", "releaseAcceptanceClaimed": False})
    print(json.dumps({"product": AGENT, "version": VERSION, "automationComplete": complete,
                      "visualReviewComplete": False}))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        # No exception text/tracebacks: protocol bodies and endpoints are private.
        print("Code Intelligence 0.5.0 owned browser acceptance failed", file=sys.stderr)
        raise SystemExit(1)
