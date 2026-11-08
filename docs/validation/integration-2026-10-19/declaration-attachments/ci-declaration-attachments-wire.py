from pathlib import Path
p=Path('native/src/changes.rs');raw=p.read_bytes();old=b'"typescript" | "tsx" | "javascript" | "jsx"';assert raw.count(old)==1;raw=raw.replace(old,old+b' | "rust" | "python"');p.write_bytes(raw)
