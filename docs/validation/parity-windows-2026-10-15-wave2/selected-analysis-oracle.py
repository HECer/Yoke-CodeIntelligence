from collections import Counter
import json
from pathlib import Path
import sqlite3
import subprocess

repo = Path(__file__).resolve().parent.parent
root = repo / "target" / "selected-analysis-oracle-fixture"
root.mkdir(exist_ok=True)
files = {
    "main.rs": "fn helper(){helper();}\nfn caller(){helper();helper();}\n",
    "guide.md": "# Guide\n`main.rs::helper`\n`main.rs::helper`\n",
    "package.json": '{"name":"app","description":"app","dependencies":{"external":"^1"}}',
}
for name, text in files.items():
    (root / name).write_text(text, encoding="utf-8")
run = subprocess.run(
    [str(repo / "target" / "debug" / "yoke.exe"), "--root", str(root), "graph", "build"],
    capture_output=True, text=True, check=True,
)
summary = json.loads(run.stdout)
view = json.loads((root / ".yoke" / "blobs" / summary["artifactId"]).read_bytes())
db = sqlite3.connect(root / ".yoke" / "index.sqlite")
graph = json.loads(db.execute("SELECT value FROM graphs WHERE id=?", (summary["snapshotId"],)).fetchone()[0])
nodes = {n["id"]: n for n in view["nodes"]}
rows = []
for observation in view["observations"]:
    row = dict(observation)
    row["sourceName"] = nodes[row["source"]]["name"]
    row["targetName"] = nodes[row["target"]]["name"] if row.get("target") else None
    rows.append(row)
result = {
    "builtBinary": str(repo / "target" / "debug" / "yoke.exe"),
    "snapshotId": summary["snapshotId"],
    "viewId": summary["viewId"],
    "canonicalByRelation": dict(Counter(o["relation"] for o in graph["observations"])),
    "structuralViewByRelation": dict(Counter(o["relation"] for o in view["observations"])),
    "structuralViewObservations": rows,
}
out = repo / "target" / "selected-analysis-oracle.json"
out.write_text(json.dumps(result, indent=2), encoding="utf-8")
print(json.dumps(result, indent=2))
