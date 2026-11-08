Design only. No streaming guard implementation is released by this file.
Root must observe the three genuine captured tests fail for the intended
behavior before releasing the corresponding parser bodies. The Object-only
candidate `13189b2d00a2dc6bc9b539987710ea6b38d982fdc6ff5f88f5cd3d54abeda0f8`
stays ignored because it cannot account for a valid array before a later
syntax failure prevents completion of its enclosing shadow tree.

The public decoder API and all finite grammars stay unchanged. Keep one caller
MarkerDecodeLedger across markers and give the private parser Guard a mutable
borrow of it. A private role accompanies each parsed value:

- Root means the document's root value. An array at Root does not confer any
  role on its children.
- NpmWorkspace means npm's root `workspaces` value. Its direct array is a raw
  selector array. If it is a map, only its immediate `packages` value has the
  Selectors role.
- CargoWorkspace means Cargo's root `workspace` map. Only its immediate
  `members` and `exclude` values have the Selectors role.
- Selectors means the value of a recognized selector-list field. It admits
  accounting only when the actual value is an array.
- Other means unrelated data and all children of array elements. The spelling
  `workspaces`, `workspace`, `packages`, `members`, or `exclude` inside Other
  never confers a new role.

Root field transfer is exact and depends on MarkerKind: npm `workspaces` to
NpmWorkspace; Cargo `workspace` to CargoWorkspace; pnpm `packages` to Selectors.
Other root fields transfer to Other. Map field transfer uses the decoded exact
UTF-8 key, preserving JSON escape equivalence and TOML quoted/dotted-key
semantics, with no case or Unicode normalization. Role transfer is structural;
an array of maps cannot masquerade as a workspace map.

For each completed direct element of an array with selector accounting enabled,
check the inherited deadline, then debit raw entry count and original decoded
string UTF-8 bytes before retaining the element in the array. A non-string
element debits one entry and zero string bytes, matching the existing schema
behavior. Children of that element receive Other and cannot debit nested
strings. The debit includes a pnpm leading `!` before any stripping or
deduplication. Existing limits and static hard codes remain 2,048 entries,
512 bytes each, and 256,000 aggregate bytes. Failed debits do not raise counters
past their ceiling or admit the extra element. Parsing an incomplete element
does not invent a raw selector entry.

This happens before insertion into an enclosing map, before any duplicate-key
collapse, before schema/policy selection, and before TOML's strict general
validator. Therefore an earlier complete array of 2,049 entries produces the
typed Capacity("selectors") even if a later token is invalid. Duplicate array
fields each debit their actual entries once, including duplicate workspace
objects and repeated nested `packages` fields. Ordinary arrays debit once.
Remove the existing post-tree charge_selectors pass so it cannot charge twice.
The existing map can discard an overwritten node after its raw entries have
been charged; duplicate-preserving Objects are unnecessary for this approach.

JSON can pass the role directly through json_value, map-value recursion, and
array recursion. json_document supplies Root only for its root value. Every
array element's own value parser receives Other; the enclosing array alone
charges a completed element when its role permits it. Duplicate-key detection
and source_syntax/source_schema classification stay unchanged below ceilings.

TOML's bounded structural parser must do the same work before
toml::from_str. For a top-level assignment, resolve the active table path plus
the dotted key prefix against the current shadow. Have table_at return the
target map's role as well as its depth. Following a map field uses the transfer
rules above; following an array-of-tables always changes the role to Other.
The assignment's final decoded key determines its value role. Inline maps
start at their enclosing value role and propagate through dotted keys in the
same way. Thus `[workspace] members`, `workspace.members = [...]`, and
`workspace = { members = [...] }` all charge correctly. Data under
`package.metadata`, `other.workspace`, workspace `default-members`, or
`[[workspace]]` does not become a supported selector declaration. Keep the
current node/depth preflight and strict TOML syntax check; duplicate TOML keys
under the raw ceilings still report source_syntax.

YAML's pull-event builder can pass the role through map values and sequence
elements with the same transfer rules. Its scanner prepass must retain a
source-syntax failure as a pending flag instead of immediately returning it:
the bounded event parser must first be allowed to process the valid prefix and
observe an earlier raw selector ceiling. Keep scanner deadline/depth guards
hard and keep recording tag/anchor/alias/merge features without expansion.
If no hard error occurs, return the existing static syntax/schema/unsupported
feature classification. An undefined alias after a completed over-limit array
cannot erase its debit; an alias before such an array does not fabricate
unobserved entries. Do not restart parsing under a fresh deadline or ledger.

The new tests are appended to the existing retrieval_scopes_tests module to
reuse genuine Engine capture and checked-run helpers. Each format has a
working two-package constructor control, a one-selector array followed by a
deliberate source_syntax control, and 2,049 complete selectors before the same
later syntax error requiring exact Capacity("selectors"). All sources are
well below one marker's byte ceiling and 20,000 nodes, every selector is three
bytes, and neither aggregate selector bytes nor per-selector bytes can be the
cause. JSON ends after `"broken":`; TOML and YAML end after an unfinished
unrelated array. No saved authority is fabricated and no parser error text is
used as an oracle.

After the actual RED release, the repair should be reviewed against the
existing duplicate-array cases, exact 2,048-entry controls, malformed-source
static codes, nested unrelated same-name data, and cross-marker cumulative
ledger semantics. Root owns compilation and execution. No Cargo/Node command,
normal-source edit, new dependency, or streaming body has been performed by
this design-only task.
