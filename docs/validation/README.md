# Validation evidence

`report.json` is the authoritative current-version summary. `tests.tap`/`tests.json` describe the default suite; `live-native.tap` contains genuine installed-provider assertions; `live-communities.tap` includes a separately labeled actual NetworkX test. Do not add the entire community file to the default total because it also reruns existing tests.

`upstream-attempts.tap` records missing installed providers as failed prerequisites, not successful compatibility. `browser.json` distinguishes the mocked DOM test from a blocked real navigation attempt. `benchmark.json` is synthetic retrieval; it does not measure an agent, API bill or upstream comparison. `configured-smoke.json` exercises a real local CLI rename/verify/isolated-apply workflow.

Versioned subdirectories contain historical runs only. A prepared workflow is not an executed job. Package integrity, source hashes and runtime extraction checks are recorded separately in the external release validation JSON to avoid a self-referential archive hash.
