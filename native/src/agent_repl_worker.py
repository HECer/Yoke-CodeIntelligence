"""Independent persistent tool-programming worker, owned by the Rust host.

Trusted Python is ordinary host execution. The RPC facade is not an OS sandbox.
"""
import ast
import contextlib
import io
import json
import math
import re
import sys

_input = sys.stdin.buffer
_output = sys.stdout.buffer
_MAX_FRAME = 4_000_000
_request_id = 0
_call_id = 0
_methods = {}
_all_methods = {}
_api_selection_active = False
_output_limit = 0


def _read():
    line = _input.readline(_MAX_FRAME + 2)
    if not line or len(line) > _MAX_FRAME or not line.endswith(b"\n"):
        raise RuntimeError("Invalid or oversized host frame")
    return json.loads(line)


def _send(value):
    encoded = json.dumps(value, ensure_ascii=True, allow_nan=False, separators=(",", ":")).encode("utf-8")
    if len(encoded) + 1 > _MAX_FRAME:
        raise RuntimeError("Worker frame budget exceeded")
    _output.write(encoded + b"\n")
    _output.flush()


class _BudgetExceeded(BaseException):
    pass


class _Output(io.TextIOBase):
    def __init__(self, budget):
        self.budget = budget
        self.parts = []

    def write(self, text):
        if type(text) is not str:
            raise TypeError("Output must be text")
        if len(text) > _output_limit - self.budget[0]:
            self.budget[1] = True
            raise _BudgetExceeded()
        size = len(text.encode("utf-8", errors="replace"))
        self.budget[0] += size
        if self.budget[0] > _output_limit:
            self.budget[1] = True
            raise _BudgetExceeded()
        self.parts.append(text)
        return len(text)

    def flush(self):
        pass

    def value(self):
        return "".join(self.parts)


def _rpc(method, arguments, project=None):
    global _call_id
    _call_id += 1
    _send({"event": "call", "requestId": _request_id, "callId": _call_id,
           "method": method, "arguments": arguments, "project": project})
    reply = _read()
    if reply.get("event") != "reply" or reply.get("requestId") != _request_id or reply.get("callId") != _call_id:
        raise RuntimeError("Host reply identity mismatch")
    if reply.get("error") is not None:
        raise RuntimeError(reply["error"])
    return reply.get("result")


class _ProjectContext:
    def __init__(self, project):
        self.project = project

    def __enter__(self):
        _rpc("bridge.ext.project_context.enter", {"project_name": self.project})
        return _Facade(self.project)

    def __exit__(self, *_):
        return False


class _Group:
    def __init__(self, group, project):
        self.group = group
        self.project = project

    def __dir__(self):
        return [name.split(".", 1)[1] for name in _methods if name.startswith(self.group + ".")]

    def __getattr__(self, name):
        method = self.group + "." + name
        parameters = _methods.get(method)
        if parameters is None:
            raise AttributeError(name)

        def invoke(*args, **kwargs):
            if len(args) > len(parameters):
                raise TypeError("Too many positional arguments")
            values = dict(zip(parameters, args))
            for key, value in kwargs.items():
                if key not in parameters or key in values:
                    raise TypeError("Unknown or duplicate argument: " + key)
                values[key] = value
            if method == "ext.project_context":
                return _ProjectContext(values["project_name"])
            if method in ("lsp.find_declaration", "jb.find_declaration"):
                source = _rpc("bridge." + method + ".source", values, self.project)
                expression = re.compile(values["regex"], re.MULTILINE | re.DOTALL)
                if expression.groups != 1:
                    raise RuntimeError("Declaration regex requires exactly one capturing group")
                encoded = source["text"].encode("utf-8")
                selected = encoded[source["startByte"]:source["endByte"]].decode("utf-8")
                match = None
                for candidate in expression.finditer(selected):
                    if match is not None:
                        raise RuntimeError("Declaration regex must select one unambiguous occurrence")
                    match = candidate
                if match is None or match.start(1) < 0 or match.start(1) == match.end(1):
                    raise RuntimeError("Declaration regex has no nonempty symbol occurrence")
                offset = source["startByte"] + len(selected[:match.start(1)].encode("utf-8"))
                return _rpc("bridge." + method + ".at",
                            {"path": source["path"], "snapshotId": source["snapshotId"],
                            "contentHash": source["contentHash"], "offset": offset,
                            "includeBody": values.get("include_body", False), "includeInfo": values.get("include_info", False)}, self.project)
            return _rpc(method, values, self.project)

        return invoke


class _Facade:
    def __init__(self, project=None):
        for group in {name.split(".", 1)[0] for name in _methods}:
            setattr(self, group, _Group(group, project))
        self._project = project

    def __getattribute__(self, name):
        value = object.__getattribute__(self, name)
        if _api_selection_active:
            if name == "invoke" or (type(value) is _Group and not any(
                    method.startswith(value.group + ".") for method in _methods)):
                raise AttributeError(name)
        return value

    def __dir__(self):
        names = object.__dir__(self)
        if not _api_selection_active:
            return names
        attributes = object.__getattribute__(self, "__dict__")
        return [name for name in names if name != "invoke" and not (
            type(attributes.get(name)) is _Group and not any(
                method.startswith(name + ".") for method in _methods))]

    def invoke(self, operation, arguments=None):
        return _rpc("native." + operation, {} if arguments is None else arguments, self._project)


def _safe(value, seen=None, depth=0, count=None):
    """Only inspect exact builtin containers/primitives. Never call repr or descriptors."""
    if seen is None:
        seen, count = set(), [0, 0]
    count[0] += 1
    count[1] += 32
    if depth > 24 or count[0] > 2000 or count[1] > _output_limit:
        return {"omitted": "result_capacity"}
    kind = type(value)
    if value is None or kind is bool:
        return value
    if kind is int:
        return value if value.bit_length() <= 4096 else {"omitted": "oversized_integer"}
    if kind is float:
        return value if math.isfinite(value) else {"omitted": "non_finite_number"}
    if kind is str:
        if len(value) > _output_limit - count[1]:
            return {"omitted": "oversized_string", "characters": len(value)}
        encoded = value.encode("utf-8", errors="replace")
        count[1] += len(encoded)
        if count[1] > _output_limit:
            return {"omitted": "oversized_string", "bytes": len(encoded)}
        return encoded.decode("utf-8")
    if kind not in (list, tuple, dict):
        return {"omitted": "unsupported_object"}
    if id(value) in seen:
        return {"omitted": "cycle"}
    seen.add(id(value))
    try:
        if kind is dict:
            if len(value) > 1000 or any(type(key) is not str or len(key) > 1000 for key in value):
                return {"omitted": "unsupported_mapping"}
            count[1] += sum(len(key.encode("utf-8", errors="replace")) for key in value)
            if count[1] > _output_limit:
                return {"omitted": "result_capacity"}
            result = {}
            for key, item in value.items():
                normalized = key.encode("utf-8", errors="replace").decode("utf-8")
                if normalized in result:
                    return {"omitted": "normalized_key_collision"}
                result[normalized] = _safe(item, seen, depth + 1, count)
            return result
        if len(value) > 1000:
            return {"omitted": "result_capacity"}
        return [_safe(item, seen, depth + 1, count) for item in value]
    finally:
        seen.remove(id(value))


def _run():
    global _request_id, _methods, _all_methods, _api_selection_active, _output_limit
    boot = _read()
    if boot.get("event") != "boot":
        raise RuntimeError("Missing worker boot")
    _methods = boot["methods"]
    _all_methods = _methods
    _output_limit = boot["maxOutputBytes"]
    namespace = {"__name__": "__yoke_agent_repl__", "s": _Facade()}
    _send({"event": "ready", "schemaVersion": 1, "facadeMethods": len(_methods)})
    while True:
        request = _read()
        if request.get("event") == "close":
            return
        if request.get("event") != "execute":
            raise RuntimeError("Unknown host request")
        _api_selection_active = "selectedMethods" in request
        if _api_selection_active:
            selected = request["selectedMethods"]
            if (type(selected) is not list or len(selected) > len(_all_methods)
                    or any(type(name) is not str or name not in _all_methods for name in selected)
                    or len(set(selected)) != len(selected)):
                raise RuntimeError("Invalid host API selection")
            _methods = {name: args for name, args in _all_methods.items() if name in selected}
        else:
            _methods = _all_methods
        _request_id = request["requestId"]
        budget = [0, False]
        stdout, stderr = _Output(budget), _Output(budget)
        result, error = None, None
        try:
            module = ast.parse(request["code"], filename="<yoke-agent-cell>", mode="exec")
            final = module.body.pop() if module.body and isinstance(module.body[-1], ast.Expr) else None
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                exec(compile(module, "<yoke-agent-cell>", "exec"), namespace, namespace)
                if final is not None:
                    result = eval(compile(ast.Expression(final.value), "<yoke-agent-cell>", "eval"), namespace, namespace)
        except _BudgetExceeded:
            budget[1] = True
        except BaseException as exception:
            kind = "Exception"
            for builtin, label in ((SyntaxError,"SyntaxError"),(NameError,"NameError"),(TypeError,"TypeError"),
                                   (ValueError,"ValueError"),(RuntimeError,"RuntimeError"),(AttributeError,"AttributeError"),
                                   (KeyError,"KeyError"),(ImportError,"ImportError"),(ZeroDivisionError,"ZeroDivisionError"),
                                   (AssertionError,"AssertionError"),(OSError,"OSError")):
                if type(exception) is builtin:
                    kind = label
                    break
            arguments = BaseException.args.__get__(exception, BaseException)
            message = arguments[0] if arguments and type(arguments[0]) is str else "Cell execution failed"
            message = message[:2000].encode("utf-8",errors="replace").decode("utf-8")
            error = {"type": kind, "message": message}
        if budget[1]:
            _send({"event": "fatal", "requestId": _request_id, "message": "Cell output budget exceeded"})
            return
        _send({"event": "result", "requestId": _request_id, "result": _safe(result),
               "stdout": stdout.value(), "stderr": stderr.value(), "error": error})


if __name__ == "__main__":
    _run()
