# Explicit project activation commands

This is an independent MIT Rust implementation of the behavior described by the
[pinned primary project template](https://raw.githubusercontent.com/oraios/serena/4f2520efb128dd02e66a2302095bdbbb26ddd5a0/src/serena/resources/project.template.yml)
at lines 44–54. No GPL application code is incorporated. This source candidate
has not been built or executed; native, CLI, stdio, HTTP and platform acceptance
remain separate validation gates.

## Declaration and execution authority

Explicit YAML projection accepts `activation_command` and
`activation_command_timeout` in global, project and project-local files. Values
stay inert during discovery, projection, export, status and inspection. Contexts
and modes cannot declare hooks. Command strings must be nonempty after whitespace
checking, NUL-free and at most 8,192 UTF-8 bytes. They are never templates.

Native `HostProfile.activationCommands` records the decoded declarations:

```json
{
  "activationCommands": {
    "global": {},
    "projects": {
      "default": {
        "project": {
          "activation_command": "a reviewed exact shell command",
          "activation_command_timeout": 10.5
        },
        "local": {}
      }
    }
  }
}
```

Each field resolves global → project → local, independently of file input order.
Missing commands inherit; explicit null disables an inherited command.
Missing/null timeouts inherit; the final default is 180 seconds. Positive
fractional numbers are supported at Rust Duration nanosecond resolution;
fractional nanoseconds refuse beyond floating-point representation tolerance. Zero,
negative, quoted, nonfinite, underflowing, overflowing and unrepresentable
durations refuse. The requested duration must fit a u64 nanosecond receipt and
the host monotonic clock. No budget is renewed to accommodate a command.

Absent fields and the prior YAML null-command/null-or-integer-180-timeout cases
retain the original exported bytes, session fingerprint and ordinary RAW reply.
Original YAML bytes and original source SHA256 receipts remain authoritative;
decoded command hashes do not claim YAML byte spans. A selected declaration gets
its own versioned activation-configuration fingerprint in the session binding.
The rest of the host profile retains its existing independent session bindings.

Neither this profile, serialized selections, language-server trust nor a model
request constructs a runtime grant. The Rust host separately calls
`project_activation::Runtime::authorize`. The returned type has no serialization,
deserialization or default constructor. The shipped CLI requires **both**:

```text
--allow-project-activation --activation-selection /absolute/selections.json
```

The private selection is a bounded JSON array, for example:

```json
[
  {
    "project": "default",
    "root": "C:/selected/project",
    "commandSha256": "<SHA256 of the exact decoded command UTF-8>",
    "configurationSha256": "<activationConfigurationSha256 from the inert plan>",
    "shell": {
      "executable": "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
      "kind": "powershell"
    }
  }
]
```

`configurationSha256` binds the entire selected activation configuration,
including its distinct layers, rather than just the effective command. A grant
also binds the exact project registration, command string, canonical physical
root, held root/ancestor identities and selected shell identity. Unknown or
query-only projects, changed pins, replacement roots and links/reparse points
refuse. Root/shell paths are bounded to 4,096 OS-string units and 128 components.
The host must explicitly select a regular physical shell path; no repository
executable discovery, trust glob, install, account or model prerequisite exists.

Supported invocation forms are `posix` (`-c`, exact command as one argument) and
`powershell` (`-NoLogo -NoProfile -NonInteractive -Command`, exact command).
The host selects the executable; the implementation never infers it from source
or language-server settings. Shell payloads are not read or hashed by this
policy. File identity, length and modification metadata are held and rechecked.

The existing process-capture environment allowlist is captured at authorization
and explicitly fixed for every attempt, including absent entries. CI,
DO_NOT_TRACK and NO_COLOR retain the process module's fixed values. No YAML or
protocol field adds environment variables. Environment contents are not saved;
only the authority binding hash reaches receipts.

## Lifecycle

`Session::open` and `open_with_git_projects` are actual host lifecycle starts.
After complete host/state/mode admission, they run the current selected project's
hook before new active-state publication. A resumed host lifecycle reruns its
current project's hook before allowing fresh backend work. A passive
`Session::inspect` never runs hooks or publishes a new selected session row.

Each live handle binds backend readiness to the exact latest durable attempt
witness and rechecks both the live and named state namespace before admission,
including cached engines and projects without a command. A newer resume attempt
revokes an older handle's admission even when the session row stays unchanged.
Only the handle that has proved its managed capture and owned-tree cleanup
may temporarily admit its pending witness while publishing the selected state. Failed attempts retain
uncertainty; passive inspection remains available without executing a hook.

The CLI `session --requests` route uses passive inspection for batches containing
only the documented passive operations: configuration, project/onboarding lists,
receipts, instructions, structural source/symbol reads, note reads/lists and
restricted project reads. Semantic backend requests refuse on a selected-hook
inspection handle. Include an explicit `activate` in a lifecycle batch when
activation is intended. Unknown or mutating operations are not passive.

`serve-session` creates the selected lifecycle before serving native/Serena tools
or starting its optional Python REPL. `agent-repl` uses the same startup path
before its interpreter starts. HTTP Session/Serena targets create the same
lifecycle during initialization. Principal admission only inspects and validates
the selected grant; it never runs a hook. The core eight-tool `native` HTTP profile
refuses activation configuration. HTTP grants come from host CLI selection,
never principal-file booleans or client initialization fields. Existing HTTP
credentials and permissions remain independent.

Every later explicit activation resolves the next modes, runs the selected hook,
admits the engine and conditionally publishes the new state. Repeating activation
of the same project runs another attempt even if the engine is cached. Removing
an active secondary project also uses this path for the default-project fallback.
Fixed startup contexts retain their original activation/removal restrictions.
Mode changes alone do not run hooks.

Opening an Engine creates source/state access and stores provider configuration;
it does not initialize an LSP process. The Session's common engine admission
requires successful hook readiness before backend use, including lazy secondary,
cached, native, Serena and REPL adapters. An inactive project's declared hook is
not implicitly executed by a query: backend access refuses until explicit
activation. Passive structural source reads remain possible without shell
authority. Public Rust Engine access is trusted host code, not a protocol grant.

## Capture, continuation and refusal

Commands run in the selected project cwd, with empty bounded stdin, at most
64,000 stdout bytes and 16,000 stderr bytes. The requested command deadline is
intersected with the inherited enclosing deadline. Requested and effective
durations are distinct receipt fields. Timeouts never extend the caller.

The command-local timeout can continue activation only after the existing owned
child/tree cleanup succeeds and stdin plus both complete EOF streams are proven.
A narrow private typed capture observation carries this proof. Cancellation,
output overflow, I/O failure, incomplete streams, unproven cleanup and enclosing
expiry refuse. Text containing “deadline” or “cleanup” proves nothing.
Nonzero exits follow the same complete capture requirements. Safe error logging
records the project, attempt ID and exit/timeout status without output excerpts.

The original checked-cleanup budget remains at most two seconds even when the
caller expires; there is no additional Drop wait. Only the new observation route
can drain post-termination pipes for at most 100 ms under the still-live enclosing
deadline. Legacy capture flags, caller errors and cleanup precedence are retained.
At least 250 ms must remain after complete capture for activation admission; all
subsequent checks/publication must still finish under the original deadline.
This reserve is not a scheduler or successful-publication guarantee.

Before and after the irreversible attempt, the implementation rechecks command,
configuration, held physical root/shell identities and exact session bytes. A
short SQL reservation CAS prevents cooperating contenders from entering the same
attempt. No database transaction spans external execution. Independent
existing-only read-only opens also check the selected state namespace and the
random attempt witness, rather than relying only on an old SQLite connection.
Engine admission and state publication have further root/state/deadline checks.

## Attempt receipts and external effects

One bounded last-attempt receipt (maximum 8,192 bytes) per session key lives in
existing Store metadata. It contains hashes, project/session identifiers,
revision, attempt nonce identity, phase, bounded stream counts/hashes, exit status
and requested/effective durations. No raw command, output or environment enters
this metadata or default session/configuration replies. Inert plan replies redact
activation declarations from `hostProfile` and identify that redaction; explicit
private profile export retains the command needed for later host selection.
Discovery also redacts an activation-bearing base profile from its returned
selection and marks `selectionReplayable:false`; use the original private base
with the returned file pins. Source-file declarations retain ordinary replayable
discovery selectors because their command bodies are never embedded there.

`configuration` adds `projectActivation` only for selected policy;
`activationReceipt` is an explicit inspection operation. Private raw diagnostics
are separately available only through the Rust host's
`Session::take_activation_diagnostic`. They are not a protocol operation or a
persisted raw-log store. Existing absent-policy replies stay exact.

A reserved/interrupted attempt remains `attempting`; uncertain capture, stale
postconditions or failed publication record `outcome_unknown` when bookkeeping
still fits the original deadline. Both phases prohibit automatic retry in that
session. If final bookkeeping is lost, `attempting` remains honest uncertainty.
State publication can have committed before a response/final receipt fails;
errors explicitly retain that possibility and never label it `notApplied`.
A lost successful response can be inspected through its `activated` receipt
without restarting a lifecycle. There is no automatic unknown-outcome reset;
the host must reconcile effects before deliberately choosing a new lifecycle.

A host-authorized shell can change source, host files, network or external
systems. This policy is not an OS sandbox, a no-write guarantee, a transaction or
rollback of shell effects. Core source read/write grants, plan approval,
verification and recovery remain unchanged and do not constrain arbitrary shell
effects. There is no hidden retry or exactly-once guarantee. Previously captured
source bytes and evidence remain immutable; an attempt does not rewrite them.

The new tests are authored source only in this candidate. Guarded native builds,
unit/integration tests, actual CLI/stdio/HTTP/REPL qualification, existing fault
tests, Node typecheck/tests and platform validation remain unrun here. Broad
upstream, all-settings, inventory and release certifications are unchanged.
