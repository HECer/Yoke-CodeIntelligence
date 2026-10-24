# Owned runtime cleanup and representable Unicode fixtures

The [preceding verified diagnostics](../windows-ci-temp/diagnostics.json) retain
macOS runtime cleanup failures and a source-exclusion assertion over NFC/NFD
filenames. This checkpoint corrects two concrete issues without claiming that
all macOS causes have been resolved.

The runtime owner previously issued a group kill and then called the raw child's
unbounded wait, separately from the existing ManagedChild checked lifecycle.
It now consumes the child through `terminate_checked` with the existing two-second
cleanup bound, also when process observation fails or the startup recipient has
gone away. The stop channel retains the actual result. Missing receipts are
typed cleanup uncertainty, and cleanup errors survive an earlier operation/
deadline refusal. An already closed terminal operation does not wait again for
a consumed cleanup receipt. Request deadlines, runtime authority and payload
protocols remain unchanged. This does not introduce an OS sandbox or prove
cleanup of processes which deliberately escape the owned process group.

Unix checked cleanup now accepts ESRCH only while still requiring an actual
owned raw-child exit status. Other signal failures refuse and retain their
numeric OS error. Absence of a process group alone is never an exit receipt.
Five native regression tests are added: natural exit status, active termination,
invalid cleanup budgets, plus Unix-only injected ESRCH without root exit and
EPERM refusal. The latter faults are one-shot and retain real owned cleanup.
Three tests apply on Windows and five on Unix. **They are not executed locally**;
native compilation and their execution remain hosted gates.

The Unicode exclusion fixture previously wrote composed and decomposed names
in one directory. On APFS these can identify the same file even on a
case-sensitive volume, so the second write could destroy the first fixture.
[Apple documents](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html)
that normalization variants cannot coexist there. The fixture now puts the
two exact spellings in separate ordinary ASCII subdirectories. Its exclusion
glob still selects the composed basename; the decomposed source, exact hash
and forged/resealed-policy refusal assertions remain. No normalization is
added to production source paths.

A separate native adversarial test uses create-new to observe whether a volume
admits two normalization variants. It never overwrites an alias, checks the
original bytes and requires captured paths/omissions to match actual directory
membership under the same literal decomposed exclusion. Both filesystem cases
execute assertions; neither is skipped or promoted to a capability it lacks.
This new native test is also awaiting hosted execution.

The macOS template/PDF memory failures remain open. The
[XNU limit implementation](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_resource.c)
rejects an address-space ceiling below current usage with EINVAL. This explains
one possible mechanism, not a measurement of the hosted process's mappings.
No memory ceiling, security boundary, default provider or model invocation is
relaxed. Other case-sensitive-root/timing tests also remain open.

Local checks pass:87 Python tests,368 native format roots plus a final check of
the four changed Rust files, typecheck and npm260/0/3 existing skips. These checks
do not compile or execute the modified native source. The local disk admission
still prevents a new full native build. Cargo profiles/lockfile, workflow,
Windows Temp setup, controlled test runner and TypeScript/VSIX product sources
remain exact to baseline4ea583e1. [Exact source and validation scope](result.json).

Complete Windows/macOS/native acceptance, archive live/browser, current VSIX,
main/release and complete STATUS remain open pending actual evidence.
