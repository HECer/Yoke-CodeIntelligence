# Unix cleanup-test wrapper trait correction

At source2154c13d the [Ubuntu Clippy job120907798275](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/40106033047/job/120907798275)
fails with E0277 in `native/src/process_cleanup_tests.rs`: the one-shot Unix
fault wrapper lacks Debug. The pinned process-wrap8.2.2 trait is emitted through
a macro whose supertraits require Debug. Reading only the trait body had missed
that constraint. The same failure is present in the PR's Ubuntu shard0.

The correction adds `#[derive(Debug)]` only to that Unix test wrapper. Its fault
injection, real process cleanup and all five assertions/tests remain. Production
source, workflow, lockfile/profiles, budgets, warning gates and test selection
stay exact to precedingb7a5049e. No new test merely mirrors the derivation.

Changed-file rustfmt passes. This is not a native typecheck: local full native
build admission is still closed, and the corrected Unix wrapper has not yet
been compiled/executed. Preceding Python/TypeScript results retain their original
scope. Actual hosted Clippy/test confirmation and complete native/STATUS
acceptance remain open. [Source hash and observed compiler failure](result.json).
