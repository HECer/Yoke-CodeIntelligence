# Explicit fixed-root native Git MCP host - 2026-10-23

Source `c42f5067c8819db15c3f47f111807289ee09d9ba` adds explicit trusted `--capture --serve` startup,
initial capture without unsolicited protocol stdout, and an invocation-owned host
driver for subsequent indexing. Root/program/limits/preference authority is never
decoded from MCP or repository JSON. See the [contract](../../../../native/docs/git-repository-host.md).

[index.json](index.json) pins 1,091 frozen inputs, seven product Git blobs and all
168 members of [raw.zip](raw.zip). Every member was reopened and its
size/SHA256 verified. ZIP SHA256 `836e4d781c39c6ad0c5b819d69912bb8a461ed98bb92c46176f28fa2aff7a756`;
247,896 bytes. External fixture homes/executables stay local.

| Check | Result |
| --- | --- |
| Full library | 578 passed, 7 existing ignored; one new bounded host test |
| New native Git host integration target | 5 passed, 0 ignored |
| Other 24 scoped targets | 177 passed, 2 existing ignored |
| Total 26 Rust groups | 760 passed, 0 failed, 9 existing ignored |
| All-target Clippy with warnings denied; locked build | passed |
| Typecheck; npm | passed; npm 260 passed, 3 existing skips |
| Formatting | 354 metadata roots pass; fmt-all OS206 retained |

`git-repository-host-test1` first passes one new library case, five new integration
cases, eight preference and sixteen scope cases. The final frozen gate is
`git-repository-host-qualified1`; its exact inputs also match the native live trial.
Tests cover cold no-state preflight/refusal, root/budget binding, real nested
membership refresh, stale preference refusal on reads/index and explicit restart,
historical readable source without Git, current restriction of older writable
child snapshots, unchanged catalogs and hostile tool arguments. An actual valid
captured graph is then replaced with an oversized row in an owned fixture; bounded
retrieval indexing retains its typed capacity failure without general fallback.

The native live trial completes 14 owned parent process calls. Protocol workers
inherit the parent's already assigned Job when starting actual native processes.
New startup emits exactly the expected protocol responses, performs real index
calls and preserves the eight-tool catalog. Published/current warm ordinary
server data matches, including after explicit scoped hosting and preference use.
Source records show read-only crossed child policy. Original snapshot/graph/control
rows and all source/Git file hashes remain exact.

Pinned Claude 2.1.306 reads a separately owned `.mcp.json` and private settings/home.
Actual `mcp get` reports Connected and `mcp list` succeeds for this server. This
qualifies client connection/initialization, not a Claude model/tool-analysis turn
or interactive UI behavior. No account login or model turn is requested. All Jobs
end empty, including server descendants. No system/Developer Mode/privilege or
original OS1314 fixture changes occur. The protected failed capture_options
executable/PDB stays untouched; all-target Clippy is metadata-only for that target.

Final CLI SHA256 `74906062325d2f3e97a01bd0f57aa2bdf553637e6145704b08d157cb18ef0ab2`.

Ordinary defaults remain unchanged. Session/multiproject/HTTP, other facade/client
adoption, broader formats/quality and current whole Windows/main/release acceptance
remain open. The original 353-entry assessments and summaries are unchanged. No
new product tests are claimed after documentation updates.
