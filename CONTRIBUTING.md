# Contributing

Use Node 22.16+, Git, Python 3.10+ and a Go toolchain for the full native test matrix. Install the pinned dependencies with `npm ci`. Run `npm run format`, `npm run typecheck`, `npm test` and `npm run bench` before proposing changes.

Add an executable regression test for bug fixes. For a provider update, include actual version-bound payloads, source hashes, negative cases and a live integration run; a mocked protocol peer alone is not sufficient for a live-compatibility claim. Never remove provenance to make a test easier. Do not turn unknown or incomplete results into false successes.

Changes to stored schemas need an explicit migration or safe rebuild policy. Changes to write/approval/recovery code need stale-hash, concurrent foreign edit and killed-process recovery tests. Preserve branch protection and avoid force pushes. Keep source control free of project state, secrets, generated runtime dependencies and customer source.

The repository license is MIT for original contributions. Do not copy current Serena application source into this codebase without an explicit licensing decision. Preserve third-party notices for any intentionally added code. Documentation should state tested behavior and known limitations, not aspirational claims.
