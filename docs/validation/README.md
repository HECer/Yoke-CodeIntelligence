# Validation artifacts

`report.json`, `tests.json`, `tests.tap`, `configured-smoke.json` and `benchmark.json` describe the 0.2.0 local run. The TAP log is the raw `npm run typecheck && npm test` output. The configured smoke uses real CLI processes and the native compiler, with no fake upstream provider. The microbenchmark uses a small synthetic corpus; it does not compare agents or providers.

`v0.1.0/` holds historical reports, including the old offline browser fixture. That browser report was not re-run for 0.2.0 and is not a live browser/backend integration claim.

The separate downloadable delivery validation report also records final ZIP integrity, extracted Git history and portable-runtime smoke checks performed after packaging. No hosted CI, external live provider or public repository publication is asserted by these reports.
