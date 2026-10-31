"""Pinned declaration data only; no native/provider/upstream code execution."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


def contract():
    path = ROOT / 'native/src/tool_usage_contract.json'
    return json.loads(path.read_bytes()) if path.is_file() else {}


class ToolUsageEstimatorContract(unittest.TestCase):
    def test_all_public_estimators_and_precise_counting_contract_are_pinned(self):
        value = contract()
        self.assertIn('estimators', value, 'Selected tool-usage estimators are missing')
        self.assertEqual(value['upstreamRevision'], 'ede329aaf0c824bea75455a2926f01420d7aa620')
        self.assertEqual(value['sourceBlobs'], {
            'src/serena/analytics.py': 'e12ae2ef9427964e2bd6b3130cf073fa886ad3bb',
            'src/serena/agent.py': 'e27fc397b850338f268f095706ef8d1fc242e649',
            'src/serena/tools/tools_base.py': '123d390ac4230c049c5b4d81edc940c2f1e0ed48',
            'src/serena/resources/serena_config.template.yml': '047955195f5fcf50d9fb061ca3e0bbd1e5d5ee3c'})
        self.assertEqual(value['field'], 'token_count_estimator')
        self.assertEqual(value['scope'], 'global')
        self.assertEqual(value['publicDefault'], 'CHAR_COUNT')
        self.assertEqual(value['estimators'], {
            'CHAR_COUNT': {'unit': 'unicode_scalars', 'divisor': 4, 'rounding': 'floor'},
            'TIKTOKEN_GPT4O': {'encoding': 'o200k_base', 'specialTokens': 'ordinary_text'},
            'ANTHROPIC_CLAUDE_SONNET_4': {'model': 'claude-sonnet-4-6', 'path': '/v1/messages/count_tokens', 'messageRole': 'user', 'responseField': 'input_tokens'}})

    def test_statistics_are_separate_from_native_estimates_billing_and_source_authority(self):
        value = contract()
        self.assertIn('nativePolicy', value, 'Native statistics boundaries are missing')
        self.assertEqual(value['nativePolicy'], {
            'selectionRequired': True,
            'inputRepresentation': 'canonical_json_arguments',
            'outputRepresentation': 'canonical_json_native_tool_result',
            'storedText': False,
            'sourceRetentionAuthority': False,
            'nativeByteEstimateChanged': False,
            'providerBillingChanged': False,
            'remoteTextDisclosureRequiresIndependentHostGrant': True})


if __name__ == '__main__':
    unittest.main()
