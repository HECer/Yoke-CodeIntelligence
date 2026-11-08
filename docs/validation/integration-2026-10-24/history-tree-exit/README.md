# Require the assigned history worker tree to exit before forced cleanup

The live-history CI runner previously accepted worker exit0 and an empty Job
after forced cleanup. An actual owned Python worker counterexample exits0 but
leaves its inert sleeping child behind. The preceding runner reports
activeAtParentExit=3, activeAfterCleanup=0 and complete=true. The regression
correctly fails because no refusal is raised. No native Yoke image is used.
[Preserved counterexample](red.txt).

The clean comparison initially observes activeAtParentExit=1 even without an
explicit child. Thus immediate zero is not a sufficient practical admission
rule for this host. That first comparison assertion fails and is preserved;
the observation alone does not identify the reason for the transient accounting.
[Initial clean comparison](positive-before.txt).

The corrected runner retains the immediate observation, then observes the Job
for at most one second inside the original180-second worker deadline. Success
requires activeBeforeCleanup=0 and treeExitedBeforeCleanup=true, independently
of subsequent forced cleanup. It still terminates and drains the Job on failure
and requires activeAfterCleanup=0. Forced termination cannot manufacture a
successful pre-cleanup exit proof. Native operation budgets, suspended assignment,
named membership, output/disk limits and the10-second cleanup bound remain.

Two actual Windows tests exercise the existing production runner with only its
fixed worker-script path replaced by an owned inert Python fixture. The real
worker is created suspended, assigned before resume and independently queries
membership in that exact Job, closing its query handle before continuing. The
clean tree becomes empty before cleanup and passes. The deliberate child case
has activeBeforeCleanup=2 and complete=false; forced cleanup leaves zero active
processes and the runner refuses. The fixture uses no native executable path.
It starts no analyzed project code and accesses no existing credentials.

All18 history admission/evidence tests pass, including the two new actual Job
cases. The whole Python suite separately passes103/0/0 with warnings treated as
errors; focused counts overlap and are never added. Typecheck and npm260/0/3
existing skips pass. Source/log hashes are recorded in [result.json](result.json).
Local log displays use UTF-8/LF and trim trailing whitespace, with original
UTF-16 capture hashes recorded separately.

Native Rust sources and the workflow remain exact to baseline c11511ee. There
is no native build, image read or execution locally; free disk remains below
the1.2-GB full-build threshold. These tests establish actual inert Job lifecycle
and stricter evidence admission, not archive CLI/HTTP/browser, persistent runtime,
native matrix or current VSIX qualification. All those gates and full STATUS
acceptance remain open.
