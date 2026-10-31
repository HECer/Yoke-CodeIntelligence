# Third-party notices

## Independently written core

Code Intelligence source, workers and tests in this repository are licensed under MIT, copyright 2026 Hermann. The implementation references public interfaces and documented concepts; it does not vendor Graft, Graphify, Serena or SolidLSP source. No GPL Serena application code is included.

## Runtime and development dependencies

The native Rust build links the exact dependency graph pinned in `Cargo.lock`, including Tree-sitter grammars, SQLite through rusqlite, Leiden, PDF extraction and cryptographic/protocol utilities. `scripts/package-native.py` produces a host-target dependency inventory and copies the full license texts into every portable native archive. The inventory includes build dependencies and must not be interpreted as a list of separately installed runtime programs. Packaging refuses missing required notices; exact-revision retrieval is opt-in. Own Rust source is MIT, and no upstream application source is vendored.

Explicit local tool-usage token counting additionally links MIT-licensed
`tiktoken-rs` 0.12.0 (copyright Roger Zurawicki), including its compiled OpenAI
`o200k_base` vocabulary. The pinned Cargo graph and normal portable dependency
license inventory apply; selecting it makes no runtime tokenizer download.
The full upstream license is at
[tiktoken-rs v0.12.0](https://github.com/zurawiki/tiktoken-rs/blob/v0.12.0/LICENSE).
Native tool-statistics code is independently written; no GPL Serena analytics
implementation is included.

The TypeScript runtime dependencies below are retained separately. A native-only VSIX contains the native inventory/license directory; adding `--include-typescript` also includes the TypeScript runtime and its notices.

| Component | Version | License | Distribution |
|---|---|---|---|
| TypeScript | 5.8.3 | Apache-2.0 | npm runtime dependency; copied with its LICENSE.txt and ThirdPartyNoticeText.txt in the portable runtime archive |
| @types/node | 25.1.0 | MIT | development-only type definitions |
| undici-types | 7.16.0 | MIT | development-only transitive type definitions |

Node.js, Python, Go, Git, GitHub CLI, Playwright and Chromium are installed tools, not bundled binaries. The browser screenshot uses system fonts; no font files are distributed.

## Optional, separately installed integrations

Graft: MIT; version 0.21.1, commit fe30ead39d5e6f0c921018d364da2bdbc9d4b3ad. Graphify: Apache-2.0 with applicable legacy MIT notices; graphifyy 0.9.87, commit 7536dc0665dba4adacca1a323c84fedb217ddc5d.

Serena's documented current application license is GPL-3.0-or-later; SolidLSP is separately MIT. The last MIT Serena application release is identified upstream as v1.7.0 / mit-final. This repository only supplies an independently written external-protocol adapter and does not redistribute either component. Adding their source/binaries later requires compliance with the actual component licenses. A process boundary should not be treated as an automatic legal exemption for a particular combined distribution.

Attribution links and version pins are in [UPSTREAMS.json](UPSTREAMS.json). Provider installation is not automatic. No association, endorsement or equivalence with upstream commercial/cloud features is claimed.
