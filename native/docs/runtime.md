# Python runtime sessions

`runtime::Session` provides an optional persistent debugger using a host-selected
Python 3.10+ interpreter and the standard library's `bdb`. Python is required;
there is no simulated execution or empty-success fallback.

The host supplies `RuntimeSpec` with an absolute interpreter path,
`trust_runtime`, separate `allow_evaluation`, and a session output limit. The
interpreter is canonicalized and must be outside the analyzed workspace. The
spec is a Rust host configuration, not a deserializable source instruction.
Requests cannot grant either authority. The CLI accepts bounded typed JSON
requests with explicit `--trust-runtime` and, for evaluation, `--allow-evaluation`.
MCP does not acquire execution authority from source or request text.

## Source and execution

Start chooses a captured `.py` entry from an exact immutable snapshot in the
current store. It copies the snapshot into a temporary tree, verifies the whole
tree, and starts an isolated-mode interpreter with bytecode writing disabled.
The original workspace is never the execution working directory. The snapshot
must contain at most 2,000 files and 16 MB of captured bytes.

Breakpoints require a captured path, its exact content hash, and an existing
source line. They are requested line breakpoints; accepting one does not prove
that Python will execute that line. Paused frames carry snapshot IDs, full
content hashes and UTF-8 byte spans for the reported Python source line,
including Unicode and CR/LF handling. Frames outside captured Python source are
omitted. A filename and line reported by Python are runtime metadata, not an
attestation of where dynamically compiled code originated.

Every successful response verifies the entire copied inventory and all source
hashes. Source additions, removals, writes or links invalidate the response and
terminate the session. Results identify themselves as
`source_bound_runtime_observation` and set `verificationAttestation: false`.
Completing a script is not a test verdict or a proven verification result.

## Pauses and requests

The session first pauses at a captured source line. `Stack`, `Variables`,
`Breakpoints`, `Step`, `Next`, `Continue` and `Close` operate on that persistent
session. Frame numbers refer to the latest pause, with the current frame first.
Stacks contain at most 20 captured frames. Variable inspection includes at most
100 local names, bounded primitive displays, container sizes and opaque object
labels. It does not invoke custom `repr`, descriptors or metaclass equality.
Names and values come directly from dictionary or intrinsic frame storage;
inspection avoids custom dictionary accessors and colliding-key equality.
Dictionary subclasses expose their backing storage, with
`variableSemantics: dictionary_storage_no_mapping_hooks`; this does not promise
their custom lookup semantics. Native optimized frame storage uses its cached
intrinsic API, including CPython's frame locals proxies. Other custom locals
mappings are explicitly reported as unsupported with `inspectionComplete:
false` and `operationCompleted: false`, without calling their methods. The
session remains paused; the CLI preserves these events and returns failure.
Omitted variable names also mark inspection incomplete. Reported code names
and filenames of at most 4,096 characters use builtin string access even when
Python preserves a string subclass in a code object. These supported hook
boundaries do not certify forged interpreter internals or a hostile process.

`Evaluate` requires separate host evaluation authority and uses the selected
frame's globals and locals. `Repl` executes statements in that frame's module
globals. It does not promise mutation of CPython fast locals. Evaluation can
execute arbitrary Python and has the same host permissions as the script.
Evaluation exceptions are operation failures; uncaught script exceptions are
program outcomes with `programCompleted: false`. Transport, protocol, output
and integrity failures are errors that close the session. `state()` returns the
last observed response; it is not a live process-health query.

## Bounds and cleanup

One lifetime deadline covers setup, execution, pauses and requests. It is not
renewed by stepping. A dedicated process-owner thread enforces the deadline
while the caller is idle. Input writes occur on a separate bounded queue so a
blocked worker cannot block the caller past the deadline. Each command is at
most 64 KiB (the initial manifest is at most 8 MB). Response frames are at most
256 KiB, lifetime framed output at most 8 MB, and source stdout/stderr at most
the host-supplied limit of 1–4,000,000 bytes. Raw stderr has a separate 32 KiB
cap. Requests and expression/statements are bounded; expression/statements are
at most 10,000 UTF-8 bytes.

Python-level source stdout and stderr are separate framed output events;
printing JSON does not create a debugger response. Source stdin is empty.
Direct writes to the protocol's stdout file descriptor invalidate framing.
Close, terminal outcomes, errors, deadline expiry and dropping the session
terminate the managed process group or Windows Job Object. Successful terminal
responses require confirmed managed cleanup.

Cleanup retains its separate bounded host budget after cancellation or expiry;
that budget does not renew the session lifetime. Before delivering a successful
response, the host checks the original lifetime deadline again after cleanup.
Missing or refused cleanup receipts remain errors, even when the lifetime has
also expired. Dropping an already stopped session does not begin another wait.

The CLI closes a still-paused session through the existing typed `Close` request
after its finite user-request batch, including an empty batch. `Close` requires
an exact `closed` response; a paused or other response is refused with checked
cleanup before state publication. Shutdown failure prevents successful batch
output. This host shutdown response is not appended
to the original user-request events; their order and values remain observations
from the requested operations. A terminal user response already confirms cleanup
and needs no additional close.

This is trusted host execution, not an OS sandbox. Python imports host standard
and installed modules, may access files and networks, and can deliberately
modify the original workspace using an explicit path. Copy verification detects
changes to the execution copy; it does not prevent arbitrary host effects.
Stepping is for the main Python thread. Managed descendant cleanup does not
guarantee containment of independently detached Unix processes. These limits
must be considered when the host grants runtime or evaluation authority.
