import argparse
import hashlib
import json
from pathlib import Path
import re

from acceptance import ROOT, EVIDENCE, source_manifest

parser = argparse.ArgumentParser()
parser.add_argument('tag')
args = parser.parse_args()
output = EVIDENCE / args.tag
totals = [0, 0, 0]
failures = []
groups = 0
logs = sorted(output.glob('*.log'))
for path in logs:
    data = path.read_text(encoding='utf-8')
    for result in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', data):
        groups += 1
        for index, count in enumerate(result):
            totals[index] += int(count)
    failures.extend(line for line in data.splitlines() if line.startswith('test ') and line.endswith('FAILED'))
before_path = output / 'source-before.json'
before = json.loads(before_path.read_text(encoding='utf-8'))
after = source_manifest()
print(json.dumps({
    'passed': totals[0], 'failed': totals[1], 'ignored': totals[2],
    'resultGroups': groups, 'logFiles': len(logs),
    'sourceFiles': len(before),
    'sourceManifestSha256': hashlib.sha256(before_path.read_bytes()).hexdigest(),
    'sourceChanges': sorted(name for name in before.keys() | after.keys() if before.get(name) != after.get(name)),
    'failures': failures,
    'receiptAvailable': (output / 'receipt.json').exists(),
}, indent=2))
