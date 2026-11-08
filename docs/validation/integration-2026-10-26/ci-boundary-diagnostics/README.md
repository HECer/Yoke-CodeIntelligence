# Pending inspection and ingress failure phases

Code Intelligence 0.5.0 diagnostic source
`34068f537295d966997f1747127ce62c396c20bc` adds failure evidence in two owned
integration-test helpers. It does not establish a runtime repair or native
acceptance at this source.

The historical failures are from `f8b02d4e`. The three requested shard jobs in
run 40247654159 checked out PR merge `fa1fa863`; the macOS final job in run
40247647764 checked out the push head `f8b02d4e`. Their receipts stay separate.
`hosted-failures.json` retains verified hashes, counts and bounded raw group
logs without native image bytes or credentials.

- Windows shard 6 passes three diagnostic-boundary tests and fails the fourth
  with `Request deadline exceeded`. Its one 20-second request budget includes
  Session opening and the facade call. The shared helper does not identify
  whether the partial or full inspection failed, or which inner phase expired.
  The complete group takes 35.88 seconds; that is not one request's duration.
  The existing LSP path includes index/capture, command-image hashing, source
  materialization, process/protocol work and verification. The separate 704
  runtime Session repairs do not qualify this integration path.
- macOS PR shard 1 has eight PDF-view failures. Shard 6 has 18 PDF failures
  and four prompt-memory failures. Every one of those 26 PDF failures records
  address-space setup errno 22 at 268,435,456 bytes. Prompt startup/direct-limit
  errors and downstream symptoms remain distinct observations.
- The separate push final job correctly refuses incomplete coverage: all
  eight shards have identical 1,087 selected inputs/inventory, seven are
  incomplete and shard 4 is complete. All 738 group logs match their receipts.
  Alongside the known memory failures, the ingress target passes 19 tests and
  fails its capacity case with `ConnectionReset` while reading the response.
  The historical client helper drops its retained buffer on panic, so the
  receipt proves neither absent nor complete HTTP 503 bytes.

The Windows helper now reports method, selected line range, Session-open and
facade phase, elapsed/remaining budget and full error context on failure.
The HTTP helper reports read error kind/OS code, elapsed time, retained byte
count, header boundary and whether the response starts with HTTP 503. It still
fails on a read error; these classifications never turn a reset into success.

All original test bodies/assertions match their prior source outside the two
helpers. The original 20-second request, three-second socket timeouts, exact
503 assertion, 750-ms capacity bound, thread/connection counts and durable
queue checks remain. Rust 1.94.1 formatting and whitespace checks pass. Typecheck and npm pass
(260 passed, three original skips), with natural owned-tree exit and 1,135
unchanged selected sources. `checks.json` records the actual receipts and local
stream/map hashes. Affected Rust integration execution and all-target Clippy
remain unrun locally at this source and pending in CI.

Independent read-only reviews support collecting these missing observations.
The busy response's full-close sequence has a possible late-input/reset race;
[RFC 9112 section 9.6](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.6)
describes that general transport risk. The receipt does not establish this
particular packet sequence. A production staged-close candidate needs causal
evidence and bounded tests under the existing 200-ms busy deadline, inherited
cancellation and 65,536-byte drain cap. No production transport change, extra
worker, retry, deadline refresh or memory-limit relaxation is introduced here.

Current full platform, macOS memory capability, integration, contract, main and
release qualification remain open. The earlier source-704 Windows package
checkpoint retains its own source identity and is not relabeled as this one.
