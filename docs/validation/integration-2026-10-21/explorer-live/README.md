# Authenticated native live Explorer checkpoint

`yoke --root /project explorer` starts a private loopback saved-source view.
`--allow-rebuild` separately enables manual source capture. The private fragment
capability becomes a Bearer header; request data cannot grant host permission,
choose another root or widen capture options. Reload reads the cached view only.
Rebuild requires exact expected snapshot and revision, creates an immutable native
capture and replaces the displayed server view only after checked source export.
Unchanged captures still advance the revision. Failures retain the previous view;
a capture may already have been persisted before an export failure.

The bounded single-engine transport checks Host/Origin, HTTP1.1, duplicate headers,
framing, content type, complete1024-byte request bounds, closed/duplicate-free JSON
and deadline/response budgets. A separate at-most1000ms admission bound prevents
idle speculative browser sockets from consuming a longer capture deadline.
Bootstrap and refusal responses expose no source/snapshot headers. Authenticated
views preserve original graph/source identities and source export limits.
The generated source graph runs in an opaque sandboxed iframe, has no parent DOM
access and retains its offline connect-src none policy. Static exports stay exact.

The scoped second guard passes31 Rust cases (six new live cases, five source-export
cases and twenty original export/editor HTTP regressions), all-target Clippy,
format, typecheck and npm test260/3 existing skips. The final delta changes only
two access-state labels in live HTML; Rust source/test bytes are unchanged. The
actual native binary is rebuilt and all six live tests, syntax/format and final
browser controls pass again. The old entire-native baseline is not rerun or
relabeled for this addition. Its ten original OS1314 fixtures remain unchanged.

Five actual isolated Chrome156 rounds preserve the causal work. Initial loading
failed because an unused browser preconnection occupied the listener; a real
regression test fails with timeout before the admission-bound fix and passes
afterward. Its PowerShell-captured diagnostic transcript is preserved in its
original encoding. Later page-reload automation had to follow the source iframe
into its new DevTools session; no sandbox restriction was relaxed. A final visible
missing-capability label is corrected and regenerated. Initial immediate port
cleanup probing had one false result; the original record and later verified
closed-port postflight both remain. All final jobs end empty and service ports close.

All eleven final PNGs are visually inspected: initial source, reload of old saved
bytes after a disk edit, actual pending rebuild, new source, unchanged capture,
stale revision refusal, explicit reload, full page reload, mobile, no host rebuild
grant and missing capability. For the pending-state image the controller briefly
holds a real request before dispatch and then releases it; there is no synthetic
response or product CSS override. The live workflow preserves literal Unicode/
BOM/CRLF and hostile text, exact original graph on reload, immutable old captures,
zero JavaScript exceptions and a deeply equal actual canonical JSON download from
the sandbox. Source remains inert, and one client cannot upgrade the host grant.
The final server and browser jobs all end empty. No model/analyzed source executes.

Native CLI stdout containing private capability URLs, browser/private process logs,
private ready files and fresh profiles stay outside the published evidence tree.
Public diagnostic reports redact capabilities and record no Authorization values.
Every archived file is checked against all ten owned ephemeral capability values
in UTF8 and UTF16LE and against capability-URL syntax. Zero values are published.
The ZIP preserves the included files byte-exactly; it is not all private process
stdio. T3 preview twice timed out15000ms, so owned fresh Chrome supplied the fallback.
No user browser profile, credentials, Google document or global setting is used.

[checkpoint.json](checkpoint.json) pins code/CLI scope and limitations.
[raw-index.json](raw-index.json) indexes the lossless included [evidence ZIP](raw-logs.zip).
Post-validation docs are mapped separately. An actual legacy fixture still exports
HTML byte-identical to SHA256c3b4e6bbf9b63ce55a35e5cb2dfaf16237d64723e4aeb0f5051f0ef424a4f4e0.
Broader browsers/accessibility/upstream parity, automatic watching, full STATUS/
R1-R9, Windows, hosted/main/release and authenticated Google acceptance remain open.
