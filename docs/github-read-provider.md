# Host-authorized GitHub reads

The independent Rust provider supplies Graphify's `list_prs`, `get_pr_impact`, and `triage_prs` operations through GitHub REST GET requests. Network access is disabled by default. The host must explicitly permit a bounded repository set. Caller arguments cannot supply an API origin, credential, executable, or network permission. Production uses `https://api.github.com`; redirects, proxies, and automatic retries are disabled. An explicit programmatic test profile supports numeric loopback HTTP fixtures only.

Credentials are resolved by the host's Windows Credential Manager reader. Neither credential values nor their hashes appear in results. Request counts, per-response bytes, aggregate bytes, page counts, item counts, and the shared deadline bound each operation. Pagination links must retain the authorized repository, origin, query filters, and consecutive page number. Repeated items with different content fail closed.

CI and review summaries describe observed metadata. Missing endpoints, exhausted budgets, incomplete pagination, and absent observations remain unknown. Successful observed checks do not establish required-check policy or merge readiness. Reviews from a different head are counted separately. PR head and base metadata are reread after impact inputs are collected; a changed observation cannot become an exact source impact.

Impact uses the existing immutable local snapshot, or an initial capture when none exists. A filename match alone produces path candidates. Exact hunk anchors additionally require a matching reported Git blob SHA-1, complete patch counts, and exact UTF-8 source lines. SHA-1 is used solely for Git object interoperability; native source identities remain SHA-256. Unchanged context lines do not select changed symbols. Incoming parallel graph observations retain their source evidence. All impact remains structural candidate evidence; file comparisons do not establish equality of the complete local tree and PR commit.

Triage uses the explicit host update-age threshold and records exclusions for wrong base, stale updates, and invalid or future timestamps. This update-age policy is distinct from an upstream creation-age heuristic. Reported overlapping changed paths are candidates, not proof of a merge conflict. Unavailable file inputs do not become empty-impact claims.

The Windows HTTP fixture explicitly restores blocking mode on accepted sockets, retaining read and write deadlines. A deterministic delayed-request regression reproduces the inherited nonblocking mode failure (`WSAEWOULDBLOCK`, 10035) without relying on random scheduling. The twelve focused gates passed with the default parallel runner and twenty consecutive repetitions. Zero-line deletion hunks retain a valid zero-width byte anchor and an equal start/end logical line, including an empty source file.

The implementation follows the primary [GitHub pull request REST contract](https://docs.github.com/en/rest/pulls/pulls), [checks contract](https://docs.github.com/en/rest/checks/runs), [review contract](https://docs.github.com/en/rest/pulls/reviews), and [API version contract](https://docs.github.com/en/rest/about-the-rest-api/api-versions). Transport uses pinned [reqwest 0.13.5](https://docs.rs/reqwest/0.13.5/reqwest/) with native platform certificate verification.

This provider is still under parity development. Exhaustive status pagination, richer triage ranking, output admission across large result sets, and the remaining adversarial freshness and patch matrix require further validation. No GitHub write operation is provided.

The separate [closed-PR discussion capture](../native/docs/github-discussions.md)
requires additional host text permission and supplies description, conversation,
review and review-comment observations without opening a source Store. It shares
the bounded transport and stricter pagination-filter checks. Offline digest
composition and a body-excluding handoff preview grant no network or service
write authority; actual service approval/publication remains open.
