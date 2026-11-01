"""Independent public-data contract; never import or run native/provider code."""
import hashlib
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CurrentBackendCatalog(unittest.TestCase):
    def setUp(self):
        self.contract = json.loads((ROOT / "docs/parity/current-backends-2026-10-31.json").read_bytes())
        raw = (ROOT / "native/src/server_catalog.rs").read_bytes().replace(b"\r\n", b"\n").decode()
        body = raw.split("const ENTRIES: &[Entry] = &[\n", 1)[1].split("];", 1)[0]
        self.blocks = re.findall(r"    Entry \{\n.*?\n    \},\n", body, re.S)
        self.assertEqual("".join(self.blocks), body, "Catalog data must use the admitted literal shape")
        self.ids = [re.search(r'id: "([a-z0-9_]+)"', block).group(1) for block in self.blocks]

    def test_current_public_identifiers_are_recognized_without_changing_legacy_entries(self):
        self.assertEqual(set(self.ids), set(self.contract["currentBackendIds"]))
        self.assertEqual(len(self.ids), len(set(self.ids)))
        legacy = self.contract["legacyCatalogOrder"]
        self.assertEqual(self.ids[:len(legacy)], legacy)
        self.assertEqual(hashlib.sha256("".join(self.blocks[:len(legacy)]).encode()).hexdigest(),
                         self.contract["legacyEntryBlocksLFSHA256"])

    def test_new_backends_cover_the_public_file_and_language_contract(self):
        entries = dict(zip(self.ids, self.blocks))
        for backend, expected in self.contract["added"].items():
            with self.subTest(backend=backend):
                self.assertTrue(backend in entries, "Missing declared backend: " + backend)
                for field in ("languages", "extensions"):
                    value = re.search(field + r": &\[(.*?)\],", entries[backend], re.S).group(1)
                    actual = [json.loads(v) for v in re.findall(r'"(?:[^"\\]|\\.)*"', value)]
                    self.assertEqual(actual, expected[field])


if __name__ == "__main__":
    unittest.main()
