# Closed variant-output startup correction

[Diagnostic job 123895143425](https://github.com/HECer/Yoke-CodeIntelligence/actions/runs/41004163411/job/123895143425)
tests source `d6b9da55be23f3c7810292bc3412d67884b664cc`.
[Artifact/member scope and hashes](index.json) retain all three original bounded
PNG/text-only ZIPs. No native executable/PDB was fetched or opened.

Native automation passes with six PNG/DOM pairs, raw worker/native/browser/driver
exit zero, EOF, and an empty Job before force cleanup. All six image/DOM hashes,
PNG headers/dimensions/privacy and 1,099 selected source hashes match (45 raw,
1,054 LF/CRLF-only). This source's new screenshots have **not** been visually
re-reviewed; the earlier `ed92e9f8` visual review remains separately scoped.

Serena and persistent Python both fail at `browser-start`, driver checkpoint
`phase-entry`, with no screenshots. Their roots exit 1 and each Job still has two
members before forced cleanup; all members are absent afterward. These remain
failed gates, not successful browser or independent shutdown qualification.

The Python wrapper selected new fixed evidence directories while the old driver
accepted only the native basename. Its closed start schema also had no variant.
The [controlled guard proof](start-admission-proof.json) evaluates only the actual
old/new admission code with in-memory environment/filesystem doubles: the old
source accepts native and rejects Serena/Python/HTTP outputs; the corrected code
admits each exact mapping. It launches no browser, native worker or network.
Other private startup conditions and later real-browser behavior still require CI.

The correction binds the start payload's fixed variant to its private worker
environment and the exact repository evidence path before profile creation or
Edge launch. The original Windows/CI/nonce, URL, installed Edge and fresh-profile
guards remain. The driver returns the admitted variant; both constructor and
parent receipt admission require it. Failed startup now retains and closes its
owned Node process/channel handles within the original teardown bounds.

All browser behavior after configuration is byte-exact after LF/CRLF normalization,
except the additive variant in the startup receipt and pure helper export. The
original history script, copied Job runner, screenshots, assertions, network
admission, waits, request counts and shutdown proof remain unchanged.

Actual checks pass: 80 focused Python tests, 11 in-memory Node protocol tests,
Node syntax parsing, causal guard RED/GREEN, typecheck, npm tests (266 passed,
three skipped), source-preservation and whitespace checks. These checks do not
qualify actual corrected browser runs. Fresh native/Serena/Python/HTTP CI and all
six screenshots per profile, strict macOS memory and release/main gates remain open.
