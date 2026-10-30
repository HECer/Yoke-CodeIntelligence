"""Behavioral PDF-mixed RED against the existing feedback executable; no build."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

EXE = Path('E:/Yoke-Build-Cache/outcome-learning-20261017/debug/yoke.exe')
BASE = Path(__file__).resolve().parent
POLICY = 'yoke-native-pdf-text-1:lopdf-0.42.0:direct_page_content_streams'
sha = lambda raw: hashlib.sha256(raw).hexdigest()


def pdf():
    content = b'BT /F1 12 Tf 10 100 Td (Native storage Gr\374\337e) Tj ET'
    objects = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>',
        b'<< /Length '+str(len(content)).encode()+b' >>\nstream\n'+content+b'\nendstream',
    ]
    raw = bytearray(b'%PDF-1.4\n')
    offsets = [0]
    for number, obj in enumerate(objects, 1):
        offsets.append(len(raw))
        raw.extend(str(number).encode()+b' 0 obj\n'+obj+b'\nendobj\n')
    xref = len(raw)
    raw.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for offset in offsets[1:]:
        raw.extend(f'{offset:010} 00000 n \n'.encode())
    raw.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    return bytes(raw)


class PdfMixedContract(unittest.TestCase):
    def test_native_and_pdf_frozen_text_share_literal_search_with_typed_page_reads(self):
        with tempfile.TemporaryDirectory(prefix='yoke-pdf-mixed-red-', dir='E:/Yoke-Test-Scratch') as folder:
            host = Path(folder)
            root = host/'source'
            root.mkdir()
            source = '// storage caf\u00e9 \U0001f30d\r\npub fn storage() {}\r\n'.encode()
            container = pdf()
            (root/'billing.rs').write_bytes(source)
            (root/'notes.pdf').write_bytes(container)

            def call(*args):
                return subprocess.run([str(EXE), '--root', str(root), '--timeout', '60000',
                    '--max-bytes', '4000000', *args], capture_output=True, timeout=70)

            def success(*args):
                result = call(*args)
                self.assertEqual(result.returncode, 0, result.stderr.decode('utf-8', errors='replace'))
                return json.loads(result.stdout)

            success('index')
            view = success('graph', 'build')
            raw_view = json.loads((root/'.yoke/blobs'/view['artifactId']).read_bytes())
            asset = success('media', 'inspect', 'notes.pdf')
            pin = dict(workspaceId=view['workspaceId'],snapshotId=view['snapshotId'],viewId=view['viewId'],
                viewArtifactId=view['artifactId'],baseGraphHash=raw_view['baseGraphHash'])
            selection = host/'selection.json'
            selection.write_text(json.dumps(dict(schemaVersion=2,source=pin,documents=[],
                pdfs=[dict(assetId=asset['assetId'],containerHash=sha(container),extractionPolicy=POLICY)])),encoding='utf-8')
            # Successful selection must come from CAS, with no later live reads.
            (root/'billing.rs').unlink()
            (root/'notes.pdf').unlink()
            query = dict(operation='find',query='storage',limit=30)
            result = success('mixed-view','--native-pdf-text','--selection',str(selection),
                '--query',json.dumps(query))
            self.assertEqual({item['source']['kind'] for item in result['items']},{'sourceUtf8','pdfPageText'})
            pages = [item for item in result['items'] if item['source']['kind']=='pdfPageText']
            self.assertEqual(len(pages),1)
            reference = pages[0]['source']
            self.assertEqual(reference['assetId'],asset['assetId'])
            self.assertEqual(reference['containerHash'],sha(container))
            self.assertEqual(reference['page'],1)
            decoded = b'Native storage Gr\xc3\xbc\xc3\x9fe\n'
            self.assertEqual(reference['contentHash'],sha(decoded))
            self.assertEqual(decoded[reference['startByte']:reference['endByte']],b'storage')
            read = success('mixed-view','--native-pdf-text','--selection',str(selection),
                '--query',json.dumps(dict(operation='read',source=reference)))
            self.assertEqual(read['item']['text'],'storage')
            self.assertEqual(read['item']['source'],reference)

    def test_schema2_requires_explicit_pdf_worker_permission_before_store_access(self):
        with tempfile.TemporaryDirectory(prefix='yoke-pdf-permission-red-', dir='E:/Yoke-Test-Scratch') as folder:
            host = Path(folder)
            root = host/'absent-source'
            selected = host/'selection.json'
            pin = {key:'a'*64 for key in ['workspaceId','snapshotId','viewId','viewArtifactId','baseGraphHash']}
            selected.write_text(json.dumps(dict(schemaVersion=2,source=pin,documents=[],
                pdfs=[dict(assetId='b'*64,containerHash='c'*64,extractionPolicy=POLICY)])),encoding='utf-8')
            result = subprocess.run([str(EXE),'--root',str(root),'mixed-view','--selection',str(selected)],
                capture_output=True,timeout=10)
            self.assertNotEqual(result.returncode,0)
            self.assertIn(b'mixed_view_pdf_permission_required',result.stderr)
            self.assertFalse(root.exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
