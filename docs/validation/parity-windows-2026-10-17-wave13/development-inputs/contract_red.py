"""Behavioral RED against the unchanged reviewed R4 executable, with no build."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

EXE = Path('E:/Yoke-Build-Cache/mixed-document-view-20261017/debug/yoke.exe')
BASE = Path(__file__).resolve().parent


class FeedbackContract(unittest.TestCase):
    def test_explicit_utf8_transcript_import_preserves_original_bytes_and_mines_candidates(self):
        with tempfile.TemporaryDirectory(prefix='yoke-transcript-red-', dir='E:/Yoke-Test-Scratch') as folder:
            host = Path(folder)
            root = host/'source'
            root.mkdir()
            (root/'billing.rs').write_text('pub fn invoice() {}\n', encoding='utf-8')
            command = [str(EXE), '--root', str(root), '--timeout', '60000', '--max-bytes', '4000000']
            indexed = subprocess.run([*command, 'index'], capture_output=True, timeout=70)
            self.assertEqual(indexed.returncode, 0, indexed.stderr.decode('utf-8', errors='replace'))
            text = 'User: caf\u00e9 \U0001f30d\r\nUser: tests passed for the invoice rule.\r\n'
            transcript = host/'selected-transcript.txt'
            transcript.write_bytes(text.encode('utf-8'))
            imported = subprocess.run([*command, '--allow-writes', 'feedback', 'import-transcript',
                '--input', str(transcript), '--label', 'Explicit fixture transcript', '--author', 'fixture-user'],
                capture_output=True, timeout=70)
            self.assertEqual(imported.returncode, 0, imported.stderr.decode('utf-8', errors='replace'))
            record = json.loads(imported.stdout)
            self.assertEqual(record['sourceHash'], hashlib.sha256(text.encode('utf-8')).hexdigest())
            mined = subprocess.run([*command, 'feedback', 'mine', '--transcript', record['id']],
                capture_output=True, timeout=70)
            self.assertEqual(mined.returncode, 0, mined.stderr.decode('utf-8', errors='replace'))
            result = json.loads(mined.stdout)
            self.assertTrue(result['candidates'])
            for candidate in result['candidates']:
                evidence = candidate['source']
                self.assertEqual(text.encode('utf-8')[evidence['startByte']:evidence['endByte']].decode('utf-8'), evidence['quote'])
            self.assertEqual(result['eventsCreated'], 0)
            self.assertFalse(result['semanticUnderstandingEstablished'])

    def test_feedback_write_grant_is_checked_before_selected_host_file_access(self):
        with tempfile.TemporaryDirectory(prefix='yoke-feedback-permission-red-', dir='E:/Yoke-Test-Scratch') as folder:
            root = Path(folder)/'missing-source'
            result = subprocess.run([str(EXE), '--root', str(root), 'feedback', 'record',
                '--input', str(Path(folder)/'absent-event.json')], capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'feedback_write_permission_required', result.stderr)
            self.assertFalse(root.exists())

    def test_explicit_source_bound_event_records_then_ranks_without_execution(self):
        with tempfile.TemporaryDirectory(prefix='yoke-feedback-red-', dir='E:/Yoke-Test-Scratch') as folder:
            host = Path(folder)
            root = host/'source'
            root.mkdir()
            text = '// Invoice policy caf\u00e9 \U0001f30d\r\npub fn invoice() -> u32 { 17 }\r\n'
            source = text.encode('utf-8')
            (root/'billing.rs').write_bytes(source)

            def call(*args):
                return subprocess.run([str(EXE), '--root', str(root), '--timeout', '60000',
                    '--max-bytes', '4000000', *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    timeout=70)

            def success(*args):
                result = call(*args)
                self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', errors='replace'))
                return json.loads(result.stdout)

            success('index')
            knowledge = host/'knowledge.json'
            knowledge.write_text(json.dumps(dict(kind='hypothesis', text='Invoice uses integer units.',
                paths=['billing.rs'], actor='fixture-author')), encoding='utf-8')
            target = success('remember', '--input', str(knowledge))
            view = success('graph', 'build')
            raw_view = json.loads((root/'.yoke/blobs'/view['artifactId']).read_bytes())
            pin = dict(workspaceId=view['workspaceId'], snapshotId=view['snapshotId'],
                viewId=view['viewId'], viewArtifactId=view['artifactId'], baseGraphHash=raw_view['baseGraphHash'])
            quote = '// Invoice policy caf\u00e9 \U0001f30d'
            event = host/'event.json'
            event.write_text(json.dumps(dict(schemaVersion=1, origin=pin, evaluated=pin,
                knowledgeId=target['id'], actor='fixture-reviewer', session='fixture-session', sequence=1,
                occurredAtMs=1000, outcome='success', rationale='Explicit authored fixture outcome.',
                sourceEvidence=[dict(path='billing.rs', contentHash=hashlib.sha256(source).hexdigest(),
                    startByte=0, endByte=len(quote.encode('utf-8')), quote=quote)], transcriptEvidence=[])), encoding='utf-8')
            created = success('--allow-writes', 'feedback', 'record', '--input', str(event))
            self.assertTrue(created['inserted'])
            self.assertFalse(created['executionVerified'])
            ranking = host/'rank.json'
            ranking.write_text(json.dumps(dict(schemaVersion=1, origin=pin, current=pin,
                knowledgeIds=[target['id']], asOfMs=1000, decayWindowDays=30, limit=20)), encoding='utf-8')
            result = success('feedback', 'rank', '--input', str(ranking))
            self.assertEqual(result['items'][0]['effectiveScore'], 1000)
            self.assertEqual(result['items'][0]['votingSessions'], 1)
            self.assertFalse(result['independentCorroborationEstablished'])
            self.assertEqual((root/'billing.rs').read_bytes(), source)


if __name__ == '__main__':
    unittest.main(verbosity=2)
