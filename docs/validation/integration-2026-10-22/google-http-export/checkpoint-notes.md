# Attempts and corrections

R1: all 12 targets passed, including 16 new functions. The terminal reporter
looked for a misspelled result directory; its false guardComplete indicator is
retained. Actual result.json proves completion, input hashes and all Jobs0.
The unused production import was moved to the test module before final checks.

R2: 22 library functions passed; the added HTTPS test expected a transport error
but received Request deadline exceeded. Its owned plaintext server waited for an
HTTP header terminator during a TLS handshake. The test server now responds on
the first TLS record, so the intended TLS refusal is exercised directly. Product
transport/deadline behavior and refusal assertions were not weakened.

R3: the controller was inadvertently launched before the preceding cargo fmt
terminal was observed. The exact owned controller was stopped; closing its Job
retired owned Cargo descendants. No R3 result is reused. Actual process inventory
showed no Cargo/rustc/rustfmt remaining before R4. Protected held PIDs were kept.

The first GREEN controller stopped before original CLI calls because its file
map used slash paths while RED used Windows backslashes. Independent comparison
proved all three original files retain exact byte sizes and hashes. GREEN2
normalizes only map-key separators; no fixture file or product code is changed.
The rejected attempt is preserved alongside the qualified GREEN2.

R4 and final are the authoritative frozen qualification; all earlier attempts and
the actual published-CLI RED are retained. The original RED replay is checked
byte-exact without reinterpreting its Unicode fixture. Actual new wire tests
separately exercise UTF-8 and split byte ranges.
