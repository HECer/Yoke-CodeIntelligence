# Compiler constructor, tagged-template and static receiver checkpoint

The explicit compiler/Git query now retains checker-resolved invocation kinds
for ordinary calls, new, super and tagged templates. Constructor overloads bind
through their actual class constructor symbol to original implementation bodies.
Construct-result classes are separate static result types, including union arms
and imported aliases; they are not runtime constructor identities. A Derived
result can retain an inherited Base constructor declaration. Implicit constructors
have no invented source span or calls edge, and retain explicit missing coverage.

Type-compatible constructor values have separate possibleConstruct proofs to
their captured implementation body or, when missing, to their candidate class
with explicit coverage. Overloads sharing one body are not invented as separate
selected overloads. Possible method dispatch compares both non-generic instance
types and constructor types, including generic static members and structural
interfaces. Literal class references keep direct bindings. Each possible proof
pairs with the original invocation source/target/range/kind/conditional flag;
native source, UTF-8, owner, endpoint and capacity admission remains independent.
Incoming traversal retains every separate candidate/call role and parallel quote.

The initial scoped run passes eleven functions and fails the new union oracle:
redundant-body pruning had suppressed the candidate matching the original
resolved body. Removing that pruning preserves the two distinct roles. The
original failing assertion remains. A later added overload oracle initially counted
Base and Derived owners sharing one inherited body together. It is strengthened
to require each original site exactly once per owner; no production change occurs.
Both original failing phases remain. Further controls use a private constructor
whose parameter shape otherwise matches, assert nonempty direct literal calls,
and verify one constructor body per original call despite overloads. The final
frozen guard passes twelve new-mode plus 51 project/Git regression functions,
63 total, with no ignored cases; all-target Clippy with warnings denied, format,
native build, typecheck and npm 260 passed/3 existing skips pass. All 1013 source/
control inputs remain byte-exact. Earlier 144-function counts belong to the
separately pinned preceding compiler checkpoint, not a current whole-suite run.

Real compiler cases cover inherited/explicit/implicit/private/abstract and
overloaded constructors, imported aliases, union result classes, original Unicode/
BOM/CRLF, repeated constructor sites, actual super bindings, generic static
members, incompatible constructor/types, structural static versus instance
receivers, optional calls, literal class references and method/free tagged
templates. Original source/Store state stays exact; preparation executes no
analyzed modules, models, Git or project build. Genuine Git is used only to
prepare the owned fixtures and original workflow report.

The actual Job-contained CLI repeats raw bytes, retains constructor/static/tag
roles plus two original parallel method proofs, preserves old captured-affected
stdout exactly, passes its exact JSON-plus-LF envelope and refuses one byte less
or missing trust without partial stdout. Every owned source/Git/Store file stays
byte-exact; selected executable/module files remain pinned and Jobs end empty.
The old compiler analyze stdout/artifact still matches its original baseline
byte-exactly. The [preceding checkpoint](../git-compiler-affected/README.md)
retains the original legacy baseline bytes; this archive adds the new after-run.
[checkpoint.json](checkpoint.json) records commands and exact
controls; [raw-index.json](raw-index.json) indexes every lossless member of
[raw-logs.zip](raw-logs.zip), including the original failing phase and API probe.
Post-validation status-only changes are mapped separately.

Generic instance instantiations, field/accessor/implicit execution effects,
arbitrary receiver/configuration/language forms, ownership/report semantics,
remote freshness and upstream/closed-world/runtime equivalence remain open.
The whole-native 98ffa78 baseline predates both Explorer additions and these
compiler queries; its ten original OS1314 failures and 53 ignores are unchanged.
No Developer Mode, privilege/global setting, original fixture or personal Google
document is used. Full STATUS/R1-R9, Windows/hosted/main/release acceptance stays
open. See the [contract](../../../../native/docs/git-compiler-affected.md).
