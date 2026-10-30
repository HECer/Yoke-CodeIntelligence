# Test fixtures follow actual filesystem membership

The [verified macOS diagnostics](../windows-ci-temp/diagnostics.json) contain
three filesystem setup/expectation issues: SS/sharp-s discovery, uppercase Git
metadata and Unicode workspace roots. This checkpoint changes only the two
native integration-test files, not production source, permissions or profiles.

Repository discovery now creates the second Unicode directory exclusively.
Where SS and sharp-s are distinct, all five original path expectations remain.
Where the volume aliases them, the test requires only the four actual paths,
verifies that no separately enumerated sharp-s entry exists and refuses its
uncaptured selector. Parent selection, stable repeated results, inert source,
protected state and incomplete/ungranted report assertions remain. The uppercase
metadata test uses the actual `.git` lookup behavior on its fresh volume in place
of assuming that every non-Windows volume is case-sensitive.

The semantic-workspace fixture still requires the two different Unicode strings
to have equal Unicode lowercase forms. It exclusively creates each directory
before writing its distinct source bytes. When both exist, the original two-root
symbol/hash/byte-offset checks and actual provider/callback assertions remain.
When the second creation proves a real alias, no second source is written. The
test verifies the original bytes through both spellings and absence of a second
directory entry. A selection containing the uncaptured spelling must refuse;
no empty success, provider execution or overwritten fixture is accepted. Frozen
source/graph bytes remain checked. Two test names now describe both contracts.

This is an explicit conditional assertion, not proof that an aliasing volume
supports two separate roots. No test is ignored or filtered. A passing alias
branch proves refusal for that filesystem case; it does not qualify the
distinct-root provider path there. [Apple's filesystem contract](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html)
describes case-sensitive/case-insensitive variants and Unicode normalization
aliases. Production source continues to bind exact captured path strings.

An independent [Windows filesystem probe](filesystem-probe.json) observes actual
exclusive creation, enumerated names and nonzero file identities. This volume
admits separate SS/sharp-s and capital-I-dot/i-combining-dot roots, while `.GIT`
and `.git` are the same directory. No alias is overwritten. This probe is not
native Rust execution or a macOS observation. The new native test branches await
real hosted compilation/execution. All368 native format roots and final checks
of the two changed Rust files pass. Production, workflow, Cargo/profiles and
TypeScript/VSIX source remain exact to2154c13d; its preceding Python87 and
npm260/0/3/typecheck checks retain their own recorded validation scopes.

Local native build admission remains closed below the existing1.2GB threshold.
No native image is opened or executed. Windows shell/provider, macOS memory/
runtime/timing, history live/browser, current VSIX, main/release and complete
STATUS acceptance remain pending. [Exact source and validation scope](result.json).
