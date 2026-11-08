"""Generate independent JSON schemas from pinned public signature facts."""
import ast
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def schema(node):
    if isinstance(node, ast.Name):
        return {"type": {"str": "string", "int": "integer", "bool": "boolean", "float": "number", "dict": "object", "list": "array"}.get(node.id, "object")}
    if isinstance(node, ast.Constant) and node.value is None:
        return {"type": "null"}
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        return {"anyOf": [schema(node.left), schema(node.right)]}
    if isinstance(node, ast.Subscript):
        name = ast.unparse(node.value)
        if name == "list":
            return {"type": "array", "items": schema(node.slice)}
        if name == "dict":
            return {"type": "object"}
        if name == "Literal":
            values = node.slice.elts if isinstance(node.slice, ast.Tuple) else [node.slice]
            return {"enum": [ast.literal_eval(value) for value in values]}
    raise ValueError(f"Unsupported pinned annotation: {ast.unparse(node)}")


def main():
    inventory = json.loads((ROOT / "docs/parity/serena-facade-mapping-2026-10-14.json").read_text(encoding="utf-8"))
    tools = []
    for row in inventory["tools"]:
        function = ast.parse(f"def contract({row['arguments']}): pass").body[0]
        properties = {}
        required_count = len(function.args.args) - len(function.args.defaults)
        for index, argument in enumerate(function.args.args):
            item = schema(argument.annotation)
            if index >= required_count:
                item["default"] = ast.literal_eval(function.args.defaults[index - required_count])
            properties[argument.arg] = item
        tools.append({"name": row["name"], "description": row["behavior"] + " Independent native adapter; edits produce guarded plans. Availability depends on host configuration.", "inputSchema": {"type": "object", "properties": properties, "required": [argument.arg for argument in function.args.args[:required_count]], "additionalProperties": False}, "facadeMethod": row["facadeMethod"], "sessionOperation": row["sessionOperation"]})
    output = {"schemaVersion": 1, "upstreamRevision": inventory["upstreamRevision"], "tools": tools}
    (ROOT / "native/src/compatibility_contracts.json").write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
