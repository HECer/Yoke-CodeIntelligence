# Native dashboard management development checkpoint

Code Intelligence 0.5.0, Windows-only scoped acceptance at product commit `552cd8d8a1075d4f360eb1517e1f72a5683db40e`.
The twelve product blobs were independently fetched from GitHub and matched the
qualified staged bytes. PR #7 remains an integration draft; this is not a main
merge, release, current whole-native run or full dashboard/upstream certificate.

The host can separately opt into live project/mode controls and project named-note
CRUD. Inert bounded requests cross threads; the non-Send Session executes them on
its owning stdio/HTTP/library host thread. Current session SHA, project, note head,
current tool/API policy, original operation deadline and revocation remain checked.
Creation requires absence; replacement, rename and deletion require the loaded
revision. Default absent/null management serialization remains unchanged.

Two real HTTP boundary failures have red/green evidence. On Windows, accepted
sockets inherited nonblocking mode: delayed normal headers/bodies caused error
10053. Accepted dashboard sockets now explicitly use bounded blocking I/O, as the
existing MCP-HTTP host already did. A separate incomplete-request regression
proved that a longer request timeout could outlive a shorter dashboard capability;
the accepted request now uses the earlier original expiry. See the primary
[Winsock accept contract](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept).

The final qualification has 677 passes, zero failures, seven existing ignored
workers, zero filtered functions and eight unique whole groups on 1133 unchanged
inputs: library600, main6, management8, old dashboard13, notes15, MCP-HTTP18,
protocol5 and steering12. All-target Clippy denies warnings, all 368 Cargo format
roots pass through the existing short-command helper, the CLI builds, and
`npm run typecheck` plus `npm test` pass (260 passes, three existing skips).
`cargo fmt --all` still hits Windows OS206; no formatter scope is narrowed.
Counts are scoped and nonadditive with earlier checkpoints. The ten original
Windows symlink-prerequisite failures are not reclassified by this work.

Actual CLI stdio runs cover Session, Serena and persistent trusted Python. They
observe genuine idle-input catalog notifications, same-session changes, revision
CAS, the 41-to-42 persistent runtime marker and listener closure on stdin EOF.
Actual CLI MCP-HTTP runs cover two independent sessions, idle actor management,
next-response catalog notification and deletion of one listener while the other
session stays usable. Only newly created synthetic credentials were accessed and
removed. Owned jobs, native processes and helper readers finish cleanly.

The actual native browser workflow creates a Unicode memory with inert script
text, preserves a draft against a second real HTTP client's concurrent update,
explicitly reloads, renames and deletes it, switches projects/modes and restores
host modes. At390px the actual DOM has no horizontal overflow; both source files
remain byte-exact. Collaborative snapshot capture repeatedly timed out, so the
separate pixel inspections use the exact product HTML with an explicitly
synthetic visual data source. These are not mislabeled screenshots of the native
HTTP workflow. Desktop/mobile and conflict images were regenerated and inspected,
including the corrected input text color and explicit dialog button handling.
Full keyboard/touch/screen-reader/all-browser acceptance is not claimed.

- [Checkpoint and source/CLI identities](checkpoint.json)
- [Complete selected raw index](raw-index.json) and [lossless raw archive](raw-logs.zip)
- [Desktop visual fixture](visual/desktop-final.png)
- [Mobile conflict visual fixture](visual/mobile-conflict-final.png)
- [Reproducible desktop document](visual/desktop-final.fixture.html)
- [Reproducible mobile document](visual/mobile-conflict-final.fixture.html)

Private capability URL files and redundant image base64 are excluded. Every
archived member was reopened and checked by length and SHA-256. Failed fixture
prototypes and red regressions are preserved but contribute no final pass count.
Registered-language selection, configuration files with durable recovery, explicit
raw logs, owned shutdown, tray/automatic opening and broader STATUS/R1-R9 work
remain open. No system/privilege change, default model call or analyzed-source
execution is introduced.
