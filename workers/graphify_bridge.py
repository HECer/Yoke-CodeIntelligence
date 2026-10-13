#!/usr/bin/env python3
"""Optional pinned Graphify extraction, before lossy simple-graph projection."""
import contextlib
import importlib.metadata
import json
from pathlib import Path
import sys

def main():
    data = json.loads(sys.stdin.buffer.read().decode('utf-8'))
    version = importlib.metadata.version('graphifyy')
    if version != '0.9.87':
        raise RuntimeError('Expected graphifyy==0.9.87; contract-test before changing the pin')
    root = Path(data['root']).resolve()
    paths = []
    for relative in data['sourceHashes']:
        path = (root / relative).resolve()
        if not path.is_relative_to(root):
            raise ValueError('Path outside snapshot')
        paths.append(path)
    with contextlib.redirect_stdout(sys.stderr):
        from graphify.extract import extract
        result = extract(paths, root=root)
    sys.stdout.buffer.write(json.dumps(dict(snapshotId=data['snapshotId'], sourceHashes=data['sourceHashes'],
                   version=version, rawExtraction=True, graph=result), ensure_ascii=False).encode('utf-8'))

if __name__ == '__main__':
    main()
