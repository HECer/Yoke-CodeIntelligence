# Git impact details at the host CLI

Explicit host access to the source-bound Git impact-details producer.

```
yoke --root C:/work/project --max-bytes 4000000 git digest REPORT_ID impact-details
yoke --root C:/work/project --max-bytes 4000000 git digest REPORT_ID impact-details --options C:/host/detail-options.json --format html
```

The command derives explanations from an already-published verified Git workflow
report. It reads that report's captured base/head sources and needs no live Git,
source files, model/provider, credentials or new capture. No new artifact is
published. JSON is the complete canonical library result plus LF; HTML uses the
sealed library renderer with escaped source markup.

An options file is one complete strict DetailOptions JSON object, at most128000
raw bytes, read through non-reparse host-file admission. Omitted options use
library defaults; supplied zeros remain supplied zeros. Zero quote allowances
produce explicit omissions. Exhausted source/row/wire/similarity budgets refuse.
The shared host reader admits metadata and accepted raw JSON at that128000-byte
limit, reads with a fixed64-KiB buffer, and refuses growth before accepting an
oversize chunk. A growing file can be probed beyond the limit before refusal;
there is no aggregate syscall-I/O cap promise. Existing Windows held-handle,
EOF and size-recheck safeguards remain in effect.
The global --max-bytes separately limits ACTUAL output including LF. An
insufficient envelope refuses before printing instead of clipping data or
changing options/resultId; select a larger host envelope for larger reports.

The selector rejects capture/snapshot/analyzer/provider/write flags and --path.
The new --options/--format flags refuse on legacy report/history/file digests.
Those older digests, load, render, analyze and seed retain their existing behavior.

Rename candidates are source-bound suggestions, not verified rename/entity
identity. Quotations preserve their exact source side/ranges and report any
omission or excerpt. Structural impact is not runtime proof. Broader R5, live-ref
freshness, area classifiers, PR/conflict/remote workflows and upstream equivalence
remain outside this command.
