"""Compiled public contract data only; no native/upstream program execution."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


def contract():
    path = ROOT / "native/src/tool_budgets_contract.json"
    return json.loads(path.read_bytes()) if path.is_file() else {}


class GlobalToolBudgetContract(unittest.TestCase):
    def test_public_numeric_settings_have_distinct_scoped_units_and_defaults(self):
        value = contract()
        self.assertIn("fields", value, "Global numeric tool policy is missing")
        self.assertEqual(value["fields"], {
            "tool_timeout": {"target": "timeoutMs", "unit": "seconds", "default": 240},
            "default_max_tool_answer_chars": {"target": "defaultAnswerChars", "unit": "unicode_scalars", "default": 150000}})
        self.assertEqual(value["scope"], "global")
        self.assertEqual(value["upstreamRevision"], "ede329aaf0c824bea75455a2926f01420d7aa620")
        self.assertEqual(value["sourceBlobs"], {
            "src/serena/resources/serena_config.template.yml": "047955195f5fcf50d9fb061ca3e0bbd1e5d5ee3c",
            "src/serena/tools/tools_base.py": "123d390ac4230c049c5b4d81edc940c2f1e0ed48"})

    def test_defaults_apply_to_exactly_the_existing_declared_answer_budget_facades(self):
        value = contract()
        self.assertIn("answerFacades", value, "Declared answer-budget method inventory is missing")
        old = json.loads((ROOT / "native/src/compatibility_contracts.json").read_bytes())
        expected = sorted(row["facadeMethod"] for row in old["tools"]
                          if "max_answer_chars" in row["inputSchema"]["properties"])
        self.assertEqual(len(expected), 17)
        self.assertEqual(value["answerFacades"], expected)
        self.assertEqual(value["configuredDefaultSentinel"], -1)
        facades = json.loads((ROOT / "native/src/agent_repl_contract.json").read_bytes())["methods"]
        for name in expected:
            self.assertIn("max_answer_chars", facades[name])
        self.assertNotIn("cfg.get_current_config", expected)


if __name__ == "__main__":
    unittest.main()
