"""Independent public-schema data checks; no native, controller or upstream run."""
import copy
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


def current():
    path = ROOT / "native/src/compatibility_current_sessions.json"
    return json.loads(path.read_bytes()) if path.is_file() else {"tools": {}}


def schema_accepts(schema, arguments):
    # This intentionally evaluates the closed object/required/string/oneOf subset
    # used by the two public data schemas, not the Rust argument implementation.
    if not isinstance(arguments, dict) or set(arguments) - schema["properties"].keys():
        return False
    if not set(schema["required"]).issubset(arguments):
        return False
    if any(schema["properties"][key]["type"] != "string" or not isinstance(value, str) for key, value in arguments.items()):
        return False
    alternatives = schema.get("oneOf")
    return alternatives is None or sum(set(row["required"]).issubset(arguments) for row in alternatives) == 1


class CurrentSessionContract(unittest.TestCase):
    def test_current_public_session_name_and_legacy_alias_are_both_discoverable(self):
        historical = json.loads((ROOT / "native/src/compatibility_contracts.json").read_bytes())
        overlay = current()["tools"]
        for row in historical["tools"]:
            if row["name"] not in ("activate_project", "serena_repl"):
                continue
            schema = overlay.get(row["name"], row["inputSchema"])
            self.assertIn("session", schema["properties"], "Current upstream parameter is absent from public discovery")
            base = {"project": "default"} if row["name"] == "activate_project" else {"code": "42"}
            self.assertTrue(schema_accepts(schema, {**base, "session": "owned"}))
            self.assertTrue(schema_accepts(schema, {**base, "session_id": "owned"}))
            for extra in ({}, {"session": "owned", "session_id": "owned"}, {"session": "owned", "session_id": "foreign"},
                          {"session": None}, {"session": True}, {"session": "owned", "root": "outside"}, {"\u0455ession": "owned"}):
                self.assertFalse(schema_accepts(schema, {**base, **extra}))

    def test_overlay_is_only_the_two_verified_upstream_signature_renames(self):
        value = current()
        self.assertEqual(set(value["tools"]), {"activate_project", "serena_repl"})
        self.assertEqual(value["schemaVersion"], 1)
        self.assertEqual(value["upstreamRevision"], "ede329aaf0c824bea75455a2926f01420d7aa620")
        self.assertEqual(value["sourceBlobs"], {"src/serena/tools/config_tools.py": "afe21fec0939fc788b556d3e86494a69d5dbe68d",
            "src/serena/tools/repl_tools.py": "7d097cfb4a0b660b775c6ee5f5acf71e035d04e4"})
        historical = json.loads((ROOT / "native/src/compatibility_contracts.json").read_bytes())
        for name, schema in value["tools"].items():
            expected = copy.deepcopy(next(row["inputSchema"] for row in historical["tools"] if row["name"] == name))
            expected["properties"]["session"] = copy.deepcopy(expected["properties"]["session_id"])
            expected["properties"]["session_id"]["deprecated"] = True
            expected["required"].remove("session_id")
            expected["oneOf"] = [{"required": ["session"]}, {"required": ["session_id"]}]
            self.assertEqual(schema, expected)


if __name__ == "__main__":
    unittest.main()
