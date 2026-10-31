"""Public context declaration data only; never run native or upstream code."""
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


def contract():
    path = ROOT / "native/src/context_options_contract.json"
    return json.loads(path.read_bytes()) if path.is_file() else {"fields": {}}


class ContextContract(unittest.TestCase):
    def test_missing_public_context_fields_have_scoped_native_targets(self):
        fields = contract()["fields"]
        expected = {"description": ("description", "metadata_text", False),
                    "tool_description_overrides": ("toolDescriptionOverrides", "tool_descriptions", True),
                    "single_project": ("singleProject", "boolean", True),
                    "structured_tool_output": ("structuredToolOutput", "nullable_boolean", True)}
        self.assertEqual(set(fields), set(expected), "Public context fields are not represented by the native projection")
        for key, (target, kind, context_only) in expected.items():
            self.assertEqual(fields[key], {"target": target, "kind": kind, "contextOnly": context_only})

    def test_contract_records_the_complete_context_and_mode_public_key_sets(self):
        value = contract()
        self.assertIn("contextFields", value, "Complete public context declaration is not pinned")
        common = {"description", "prompt", "excluded_tools", "included_optional_tools", "fixed_tools", "excluded_apis", "included_apis"}
        self.assertEqual(set(value["modeFields"]), common)
        self.assertEqual(set(value["contextFields"]), common | {"tool_description_overrides", "single_project", "structured_tool_output"})
        self.assertEqual(value["upstreamRevision"], "ede329aaf0c824bea75455a2926f01420d7aa620")
        self.assertEqual(value["sourceBlobs"], {
            "src/serena/resources/config/contexts/context.template.yml": "62259f8765acd5f851c2f17d41300f89c6a94d99",
            "src/serena/resources/config/modes/mode.template.yml": "bf01e71472f381e6beb9e1511c8c3ff0dcb78318"})


if __name__ == "__main__":
    unittest.main()
