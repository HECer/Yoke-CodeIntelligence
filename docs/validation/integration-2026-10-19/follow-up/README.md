# Follow-up platform and packaging checks

These scoped results follow the frozen baseline; no counts are added to it.
Hosted all-target Clippy exposed three additional Windows-only fixture details.
The imports/helper now use their actual platform scope; the temporary-directory
owner is retained under an underscore field rather than suppressing its lifetime.
No test or unavailable production capability is turned into a success.

The first optimized native build passed in 14m08s, then packaging correctly failed
because the pinned yaml-rust2 tree uses Apache-LICENSE and MIT-LICENSE names.
The packager now recognizes those closed ASCII forms in both locked crates and
pinned upstream trees. Executable/Unicode lookalikes, sibling scopes, traversal,
blob tampering and raw byte capacity remain tested refusals. Actual upstream Git
objects for both complete texts are validated, rather than replacing them with
SPDX metadata. All 17 packaging tests pass; all 29 selected native regressions
pass (one separate actual Docker gate remains ignored). Format and Clippy pass.

A prior separate follow-up passes all 18 HTTP tests and two explicitly selected
actual Windows credential / owned loopback Bedrock tests. The latter create and
remove only unique fake credentials; they make no AWS service or real model call.
All 223 recorded native/worker production build inputs remain unchanged. The
portable archive is retried after this correction; full native/platform, provider
and release acceptance remain open.
