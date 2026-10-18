"""Read only the two full state diagnostics without emitting fixture payloads."""
from pathlib import Path
import json
import re

lines = Path("target/parity-validation-20261015/r5b-private-first-20261016/run.log").read_text(encoding="utf-8").splitlines()
left = next(line[len("  left: "):] for line in lines if line.startswith("  left: "))
right = next(line[len(" right: "):] for line in lines if line.startswith(" right: "))
pattern = r'"totalChanges": Number\((\d+)\)'
print(json.dumps(dict(leftTotalChanges=re.findall(pattern, left), rightTotalChanges=re.findall(pattern, right),
                      equalAfterReplacingTotalChanges=re.sub(pattern, "TOTAL_CHANGES", left) ==
                          re.sub(pattern, "TOTAL_CHANGES", right),
                      leftBytes=len(left), rightBytes=len(right))))
