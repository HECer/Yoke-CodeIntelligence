"""Host-authored bounded bdb worker. Captured source never configures authority."""
import bdb
import hashlib
import io
import json
import os
import struct
import sys

MAX_FRAME = 262144
MAX_INPUT = 8000000
control_in = os.fdopen(os.dup(0), "rb", buffering=0)
control_out = os.fdopen(os.dup(1), "wb", buffering=0)
original_type = type
original_repr = repr
original_len = len
original_str = str
original_bytes = bytes
original_int = int
original_float = float
original_bool = bool
original_none = type(None)
original_containers = (list, tuple, dict, set)
original_dict = dict
original_issubclass = issubclass
original_sorted = sorted
original_max = max
original_any = any
function_name_slice = slice(0, 256)


def frame_storage_type():
    # CPython 3.13+ fast locals use its intrinsic FrameLocalsProxy type. Capture
    # that type's method before source runs; never dispatch a custom mapping.
    return original_type(sys._getframe().f_locals)


intrinsic_frame_storage = frame_storage_type()
intrinsic_frame_items = intrinsic_frame_storage.items


def local_storage_items(storage):
    if original_issubclass(original_type(storage), original_dict):
        return original_dict.items(storage), "dictionary_storage_no_mapping_hooks"
    if original_type(storage) is intrinsic_frame_storage:
        return intrinsic_frame_items(storage), "frame_local_storage_no_mapping_hooks"
    return None, "unsupported_custom_locals_mapping"


class WorkerFailure(BaseException):
    """Protocol/capability errors are distinct from captured program exceptions."""


def require(condition, message):
    if not condition:
        raise WorkerFailure(message)


def read_exact(count):
    result = bytearray()
    while len(result) < count:
        part = control_in.read(count - len(result))
        if not part:
            raise WorkerFailure("Debugger command stream closed")
        result.extend(part)
    return bytes(result)


def receive():
    count = struct.unpack(">I", read_exact(4))[0]
    if count > MAX_INPUT:
        raise WorkerFailure("Debugger command frame exceeds capacity")
    try:
        return json.loads(read_exact(count).decode("utf-8"))
    except (ValueError, UnicodeError):
        raise WorkerFailure("Malformed debugger command") from None


config = receive()
require(config["schema"] == 1 and config["command"] == "init", "Invalid debugger initialization")
token = config["sessionId"]
request_id = 0
allow_evaluation = config["allowEvaluation"] is True
root = os.path.abspath(config["root"])
manifest = config["files"]
by_absolute = {os.path.normcase(os.path.abspath(os.path.join(root, path))): path for path in manifest}
output_bytes = 0
output_exceeded = False
closed = False


def emit(event, data):
    message = {"schema": 1, "sessionId": token, "requestId": request_id, "event": event, "data": data}
    encoded = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(encoded) > MAX_FRAME:
        raise WorkerFailure("Debugger response frame exceeds capacity")
    for data in (struct.pack(">I", len(encoded)), encoded):
        view = memoryview(data)
        while view:
            count = control_out.write(view)
            if not count:
                raise WorkerFailure("Debugger protocol output closed")
            view = view[count:]


class OutputLimit(BaseException):
    pass


class ProgramOutput(io.TextIOBase):
    def __init__(self, stream):
        self.stream = stream

    @property
    def encoding(self):
        return "utf-8"

    def writable(self):
        return True

    def write(self, text):
        global output_bytes, output_exceeded
        if not isinstance(text, str):
            raise TypeError("Expected text output")
        size = len(text.encode("utf-8", errors="replace"))
        if output_bytes + size > config["maxOutputBytes"]:
            output_exceeded = True
            raise OutputLimit()
        output_bytes += size
        for position in range(0, len(text), 2048):
            emit("output", {"stream": self.stream, "text": text[position:position + 2048]})
        return len(text)

    def flush(self):
        pass


sys.stdout = sys.__stdout__ = ProgramOutput("stdout")
sys.stderr = sys.__stderr__ = ProgramOutput("stderr")
sys.stdin = sys.__stdin__ = io.StringIO("")


def rendered(value):
    """Never execute arbitrary __repr__, descriptors, or container element hooks."""
    kind = original_type(value)
    if kind is original_str:
        display = original_repr(value[:512])
        return {"kind": "str", "display": display, "truncated": original_len(value) > 512}
    if kind is original_bytes:
        return {"kind": "bytes", "display": original_repr(value[:256]), "truncated": original_len(value) > 256}
    if kind is original_int:
        display = "<large integer>" if original_int.bit_length(value) > 4096 else original_repr(value)
        return {"kind": "int", "display": display[:512], "truncated": original_len(display) > 512}
    if kind is original_float or kind is original_bool or kind is original_none:
        return {"kind": "primitive", "display": original_repr(value), "truncated": False}
    if original_any(kind is container for container in original_containers):
        return {"kind": "container", "display": "<container: %d items>" % original_len(value), "truncated": False}
    return {"kind": "opaque", "display": "<opaque object>", "truncated": False}


class Debugger(bdb.Bdb):
    def __init__(self):
        super().__init__()
        self.frames = []

    def source_path(self, frame):
        filename = frame.f_code.co_filename
        if original_str.__len__(filename) > 4096:
            return None
        # Code objects preserve str subclasses. Bypass their __str__ and path
        # hooks, with a bounded copy, before normalizing reported metadata.
        filename = original_str.__str__(filename)
        return by_absolute.get(os.path.normcase(os.path.abspath(filename)))

    def stack(self, frame):
        self.frames = []
        cursor = frame
        while cursor is not None and len(self.frames) < 20:
            path = self.source_path(cursor)
            if path is not None:
                self.frames.append(cursor)
            cursor = cursor.f_back
        return [{"frame": index, "path": self.source_path(f), "line": f.f_lineno,
                 "function": original_str.__getitem__(f.f_code.co_name, function_name_slice),
                 "functionNameTruncated": original_str.__len__(f.f_code.co_name) > 256}
                for index, f in enumerate(self.frames)]

    def user_line(self, frame):
        if self.source_path(frame) is not None:
            self.pause(frame, "line")

    def user_return(self, frame, return_value):
        if self.source_path(frame) is not None:
            self.pause(frame, "return")

    def user_exception(self, frame, exc_info):
        if self.source_path(frame) is not None:
            self.pause(frame, "exception")

    def pause(self, frame, reason):
        global request_id, closed
        frames = self.stack(frame)
        emit("paused", {"status": "paused", "pauseReason": reason, "frames": frames,
                        "pythonVersion": list(sys.version_info[:3])})
        while True:
            request = receive()
            require(request["schema"] == 1 and request["sessionId"] == token, "Debugger command scope mismatch")
            require(original_type(request["requestId"]) is original_int and request["requestId"] > request_id,
                    "Debugger command sequence mismatch")
            request_id = request["requestId"]
            operation = request["command"]
            result = {"status": "paused", "frames": frames}
            if operation == "stack":
                emit("result", result)
            elif operation == "variables":
                selected = self.frames[request["frame"]]
                items, semantics = local_storage_items(selected.f_locals)
                result["variableSemantics"] = semantics
                result["inspectionComplete"] = items is not None
                result["operationCompleted"] = items is not None
                if items is None:
                    result["variables"] = []
                    result["omittedVariables"] = None
                    result["errorKind"] = "unsupported_custom_locals_mapping"
                else:
                    # Re-looking up a string name can run a colliding custom
                    # key's equality method. Iterate backing values directly.
                    pairs = original_sorted(
                        ((name, value) for name, value in items if original_type(name) is original_str),
                        key=lambda pair: pair[0])
                    result["variables"] = [{"name": name[:256], "nameTruncated": original_len(name) > 256,
                                             **rendered(value)} for name, value in pairs[:100]]
                    result["omittedVariables"] = original_max(0, original_len(pairs) - 100)
                    result["inspectionComplete"] = result["omittedVariables"] == 0
                emit("result", result)
            elif operation == "breakpoints":
                self.clear_all_breaks()
                accepted = []
                for breakpoint in request["items"]:
                    path = breakpoint["path"]
                    require(path in manifest and breakpoint["contentHash"] == manifest[path]["hash"],
                            "Breakpoint scope mismatch")
                    full = os.path.abspath(os.path.join(root, path))
                    error = self.set_break(full, breakpoint["line"])
                    if error:
                        raise WorkerFailure("Breakpoint line unavailable")
                    accepted.append(breakpoint)
                result["breakpoints"] = accepted
                result["breakpointSemantics"] = "requested_lines_not_proven_executable"
                emit("result", result)
            elif operation == "step":
                self.set_step()
                return
            elif operation == "next":
                self.set_next(frame)
                return
            elif operation == "continue":
                self.set_continue()
                return
            elif operation in ("evaluate", "repl"):
                require(allow_evaluation, "Host evaluation authority required")
                selected = self.frames[request["frame"]]
                try:
                    if operation == "evaluate":
                        value = eval(request["expression"], selected.f_globals, selected.f_locals)
                        result["evaluation"] = rendered(value)
                    else:
                        # CPython fast-local mutation is deliberately not implied.
                        exec(request["code"], selected.f_globals, selected.f_globals)
                        result["replScope"] = "selected_frame_module_globals"
                        result["fastLocalsWritten"] = False
                    result["operationCompleted"] = True
                except OutputLimit:
                    raise
                except BaseException:
                    result["operationCompleted"] = False
                    result["errorKind"] = "evaluation_exception"
                emit("result", result)
            elif operation == "close":
                closed = True
                emit("closed", {"status": "closed", "frames": [], "programCompleted": False})
                self.set_quit()
                return
            else:
                raise WorkerFailure("Unsupported debugger command")


try:
    require(sys.version_info >= (3, 10), "Python 3.10+ required")
    entry = config["entry"]
    require(entry in manifest and entry.endswith(".py"), "Captured Python entry required")
    full = os.path.abspath(os.path.join(root, entry))
    with open(full, "rb") as source:
        code_bytes = source.read(manifest[entry]["size"] + 1)
    require(len(code_bytes) == manifest[entry]["size"], "Captured entry size mismatch")
    require(hashlib.sha256(code_bytes).hexdigest() == manifest[entry]["hash"], "Captured entry hash mismatch")
    sys.path.insert(0, os.path.dirname(full))
    sys.argv = [full]
    namespace = {"__name__": "__main__", "__file__": full, "__package__": None}
    debugger = Debugger()
    debugger.run(compile(code_bytes, full, "exec"), namespace, namespace)
    if output_exceeded:
        raise OutputLimit()
    if not closed:
        emit("finished", {"status": "finished", "frames": [], "programCompleted": True})
except OutputLimit:
    emit("error", {"errorKind": "output_exceeded"})
except WorkerFailure:
    emit("error", {"errorKind": "worker_protocol_or_capability_failure"})
except BaseException:
    if not closed:
        emit("finished", {"status": "finished", "frames": [], "programCompleted": False,
                          "errorKind": "program_exception"})
