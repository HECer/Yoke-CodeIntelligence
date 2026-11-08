# Preserve status-hook input bytes through the selected shell

All eight Windows shard log artifacts from source4ea583e1 in
[run40104906189](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40104906189)
were downloaded without native images or ZIP extraction. API byte lengths/SHA-256,
closed entry names/types/capacities, exact source/inventory and every log hash
are checked. [Windows evidence](windows-shards.json) records the actual whole
groups. This is not a complete successful Windows/platform or release result.

The formerly failing selected-shell source-boundary assertion passes. Its status
bootstrap still fails, but the new diagnostics narrow it: exit1 after2.064284s,
stdin/output complete, no timeout or I/O error, and independent library callback
success. The same run has separate explicit-shell REPL and managed-known-folder
failures; these remain open. Its whole library passes694/0/7 ignored/0 filtered,
which belongs only to source4ea583e1, not the later native changes.

Process.StandardInput constructs a managed StreamWriter and enables AutoFlush.
[Microsoft's Process source](https://github.com/microsoft/referencesource/blob/main/System/services/monitoring/system/diagnosticts/Process.cs)
and [writer implementation](https://github.com/microsoft/referencesource/blob/main/mscorlib/system/io/streamwriter.cs)
show why a preamble can precede later BaseStream writes. An actual isolated
PowerShell5/Python probe reproduces this:164 original binary bytes become166
under a BOM-emitting managed UTF-16 Console encoding. This demonstrates a real
transport defect, not the unmeasured encoding configuration of the hosted runner.

The generated bootstrap now inherits the existing stdin handle without creating
that redirected text writer. Separate raw stdout/stderr readers, exact native
argument quoting, selected-shell hash/source checks, NoProfile/NonInteractive,
CreateNoWindow, native20s budget, shell25s wait, output waits and owned cleanup
remain. No console/system/privilege setting or JSON parser rule changes.

The [self-contained inert probe](inert-probe.py) transfers0/164/65536 original
bytes exactly, including NUL, Ctrl-Z and Unicode, with exact Unicode/quoted
argument hashes and independent stderr. A fourth passing case reproduces the
old166-byte behavior; a fifth requires164 exact bytes with inherited stdin under
that same managed encoding. All five exit0. A suspended worker is assigned to
its fresh kill-on-close Job before resume; actual Job membership and zero active
processes after cleanup are checked. The managed UTF-16 encoding case remains
inside the child process and avoids an OS console-code-page change. No native
Yoke image is opened or executed. PowerShell7 is absent and is not qualified.
[Probe observations](inert-probe.json).

The native integration target retains its existing status bootstrap assertions
and adds the actual generated command under the BOM-emitting managed encoding.
The adversarial input test explicitly rejects UTF-8 and both UTF-16 BOM prefixes
without state changes. Another Windows process test gains only failure-time,
pipe/overflow/deadline diagnostics; its assertions and20s budget remain.
These modified native tests are not compiled or executed locally.

Local checks pass:87 Python tests,368 format roots plus final changed-file
rustfmt, typecheck and npm260/0/3 existing skips. The inert probe is separate
API evidence, not a native/Claude client or complete STATUS certificate.
Local full native build admission remains closed below1.2GB. Current native CI,
remaining Windows/macOS failures, archive live/browser, current VSIX, main/release
and full STATUS acceptance remain open. [Source/log/probe hashes](result.json).
