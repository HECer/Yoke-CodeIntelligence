# Explicit source-bearing Explorer checkpoint

The native `export --format html-source` command now exports checked immutable
captured files alongside the original canonical graph. Code, Context and Outline
tabs, exact observation quotes, keyboard tab navigation, paged source/outline and
relationship verb chips work offline. Original directions, parallel observations,
resolution labels, evidence, coverage and omissions stay distinct.

The old metadata-only HTML and document exports keep their original template.
An actual existing synthetic fixture reproduces its published HTML byte-for-byte
(`c3b4e6bbf9b63ce55a35e5cb2dfaf16237d64723e4aeb0f5051f0ef424a4f4e0`).
The actual downloaded canonical JSON remains deeply equal to the original graph.

Five new Rust tests plus ten existing export/explorer/document checks pass.
The frozen second scope passes all-target Clippy, format, typecheck and npm test
(260 passed, three existing skips). The final source delta is one CSS maximum-
height change; the native binary is rebuilt and all five source-export tests,
format, JavaScript syntax and the actual browser controls pass again.
The earlier complete native baseline is not rerun or relabeled for this addition.

Three real isolated Chrome156 rounds use only owned synthetic UTF-8/CRLF/BOM,
Unicode, hostile-text and long-file fixtures. The initial dense Outline and later
below-fold selection are corrected and regenerated. The twelve final PNGs are
visually inspected: initial view, Code, Context, exact quote, Outline, paged late
declaration, filtered neighbors, radial, Force start/middle/settled and mobile Code.
Exact UTF-8 selection and quote checks pass, keyboard tabs work, no JavaScript
exception occurs and the final selected declaration is geometrically visible on
desktop and mobile. There is no mobile horizontal overflow in this fixture.
No other viewport/browser, frame-rate or exhaustive accessibility certification
is claimed. The T3 preview twice returned `Preview automation snapshot timed out
after 15000ms.`; a fresh owned Chrome profile supplied the working fallback.
The first client controller had a duplicate local identifier before execution;
its original file is retained, corrected and run against the same live browser.
All browser/native jobs end empty and every owned server closes.

Source admission reuses canonical snapshot/graph/store/path-scope/hash/UTF-8/range
validation. Complete source bytes are admitted before opening blobs; source and
escaped final output have independent hard byte bounds. No partial result or
overwrite fallback is used. Source is text, never HTML or executable project code.
The default remains metadata-only; this explicit export contains source and needs
the same access controls as the captured project. Current disk freshness is not
checked. Live rebuild/reload still needs a separate explicit authorized workflow.
No system settings, privileges, symlink fixtures or ignores are changed.

[checkpoint.json](checkpoint.json) pins runtime source, tools and actual CLI.
[raw-index.json](raw-index.json) records every byte length and SHA-256 in the
lossless [raw ZIP](raw-logs.zip), including intermediate limitations and final
captures. Post-validation documentation changes are recorded separately.
Full STATUS/R1-R9, Windows, hosted/upstream, authenticated Google, main and release
acceptance remain open. No model or analyzed source is executed by these controls.
