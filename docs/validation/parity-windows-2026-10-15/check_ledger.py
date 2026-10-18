import hashlib
import json
from pathlib import Path

from acceptance import ROOT, EVIDENCE

draft = json.loads((EVIDENCE / 'parity-reconciliation-draft.json').read_text(encoding='utf-8'))
inventory_path = ROOT / draft['historicalInventory']['path']
assert hashlib.sha256(inventory_path.read_bytes()).hexdigest() == draft['historicalInventory']['sha256']
inventory = json.loads(inventory_path.read_text(encoding='utf-8'))
rows = [row for category in draft['inventoryLedger'].values() for row in category]
assert len(rows) == 353 == draft['ledgerSummary']['totalEnumeratedEntries']
assert len({row['pointer'] for row in rows}) == len(rows)
assert len({row['ref'] for row in rows}) == len(rows)
groups = {group['id'] for group in draft['reconciledCapabilities']}

def resolve(value, pointer):
    for component in pointer.lstrip('/').split('/'):
        component = component.replace('~1', '/').replace('~0', '~')
        value = value[int(component)] if isinstance(value, list) else value[component]
    return value

for row in rows:
    original = resolve(inventory, row['pointer'])
    digest = hashlib.sha256(json.dumps(original, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode('utf-8')).hexdigest()
    assert digest == row['pinnedEntrySha256'], row['pointer']
    assert row['fullContractCertification'] is False, row['pointer']
    assert set(row['groups']).issubset(groups), row['pointer']
    assert row['evidenceProfile'] in draft['ledgerEvidenceProfiles'], row['pointer']
    if 'currentMappingPointer' in row:
        path, pointer = row['currentMappingPointer'].split('#', 1)
        resolve(json.loads((ROOT / path).read_text(encoding='utf-8')), pointer)
for group in draft['reconciledCapabilities']:
    for key in ['implementation', 'tests', 'docs']:
        for path in group.get(key, []):
            assert (ROOT / path).is_file(), path
assert len(draft['inventoryLedger']['serenaLanguageServerIds']) == 77
assert all(row['assessment'] == 'catalog_declared_runtime_review_required'
           for row in draft['inventoryLedger']['serenaLanguageServerIds'])
print(json.dumps({'rows': len(rows), 'groups': len(groups), 'pinnedHashesValid': True,
                  'allFullContractCertifications': False, 'originalInventoryUnchanged': True}))
