"""Public declaration data checks only; no native/upstream code execution."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


def contract():
    path = ROOT / 'native/src/server_autoselection_contract.json'
    return json.loads(path.read_bytes()) if path.is_file() else {}


class CreationSelectionContract(unittest.TestCase):
    def test_creation_count_priority_and_stable_public_order_are_explicit(self):
        value = contract()
        self.assertIn('backends', value, 'Creation-time backend policy is missing')
        self.assertEqual(value['scope'], 'creation_time')
        self.assertEqual(value['selection'], 'filename_count_then_priority_then_public_order')
        self.assertEqual(value['upstreamRevision'], 'ede329aaf0c824bea75455a2926f01420d7aa620')
        self.assertEqual(value['sourceBlobs'], {
            'src/solidlsp/ls_config.py': 'd98e0a9d516b398c6b89fa4b1b38ed33fbde3cb9',
            'src/serena/resources/serena_config.template.yml': '047955195f5fcf50d9fb061ca3e0bbd1e5d5ee3c'})
        rows = value['backends']
        self.assertEqual(len(rows), 80)
        self.assertEqual(len({r['id'] for r in rows}), 80)
        self.assertEqual([r['id'] for r in rows[:3]], ['csharp', 'vbnet', 'python'])
        self.assertEqual(rows[-1]['id'], 'graphql')
        data = {r['id']: r for r in rows}
        for key in ['rust', 'python', 'csharp', 'typescript']:
            self.assertEqual(data[key]['priority'], 2)
        for key in ['vbnet', 'vue', 'svelte', 'astro']:
            self.assertEqual(data[key]['priority'], 1)
        for key in ['python_basedpyright', 'typescript_tsgo', 'graphql', 'angular', 'json', 'html', 'ansible']:
            self.assertEqual(data[key]['priority'], 0)

    def test_complete_literal_suffixes_and_case_policy_are_preserved(self):
        value = contract()
        self.assertIn('backends', value, 'Public filename match data is missing')
        data = {r['id']: r for r in value['backends']}
        self.assertEqual(data['python']['suffixes'], ['.py', '.pyi'])
        self.assertEqual(data['vbnet']['suffixes'], ['.cs', '.vb'])
        expected = sorted('.' + prefix + base + postfix for prefix in ['c','m','']
                          for postfix in ['x',''] for base in ['ts','js'])
        for key in ['typescript', 'typescript_vts', 'typescript_tsgo', 'deno']:
            self.assertEqual(data[key]['suffixes'], expected)
        self.assertIn('.app.src', data['erlang']['suffixes'])
        self.assertFalse(data['ada']['caseSensitive'])
        self.assertTrue(data['rust']['caseSensitive'])
        for row in value['backends']:
            self.assertEqual(set(row), {'id', 'priority', 'caseSensitive', 'suffixes'})
            self.assertIs(type(row['caseSensitive']), bool)
            self.assertIn(row['priority'], [0,1,2])
            self.assertTrue(row['suffixes'])
            self.assertEqual(row['suffixes'], sorted(set(row['suffixes'])))


if __name__ == '__main__':
    unittest.main()
