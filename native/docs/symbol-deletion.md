# Symbol deletion boundaries

`deleteSymbol` preparation derives a separate declaration deletion range for
TypeScript, TSX, JavaScript, JSX, Python and Rust. Structural entities and provider usage
anchors keep their original byte coordinates. This also governs the final
preparation step of symbol-selected `safeDelete` and its facade aliases.

The selected entity must bind one exact, error-free syntax node: declaration
range, name range, name text, kind, path and language all agree with captured
source. Kind binding includes the released adjustment for a function whose
nearest declared owner is a class, struct, interface or implementation. A
function nested inside a method remains a function. Stored graph kinds are
checked, not rewritten. The existing captured-source policy supplies hash-checked bytes. The
parser and bounded traversal observe the request's original deadline and host
cancellation. Input is limited to 4,000,000 bytes and 200,000 nodes per full AST traversal;
NUL, malformed source, missing bindings and capacity overflow refuse.

Named function/generator, class, interface, enum and type declarations can be
removed from program or statement-block containers. A function-valued variable
requires a single declarator in its owning `const`, `let` or `var` declaration.
An enclosing export or ambient wrapper is included only when it owns that
declaration. Named default function/class expressions require their direct
owning export. A class/interface member requires a class or type body; a
separate preceding decorator refuses until its ownership can be checked.

Multiple declarators, named expressions inside other expressions, object
literal methods and loop bindings refuse rather than removing their consumers
or siblings. Comments outside the admitted declaration and all unrelated bytes,
including BOM, Unicode and line endings, remain intact. Comments inside a
removed wrapper are part of that declaration. Insert-before, insert-after and
replace-symbol operations retain their original entity coordinates.

Python function and class definitions include their owning decorated-definition
node. The selected wrapper must directly own that exact definition. Definitions
in a required body can be removed only when another non-comment statement
remains; an otherwise empty class, function or compound-statement body refuses.
No implicit `pass` or replacement program is inserted. A recognized leading
UTF-8 BOM stays outside the deletion, with parser coordinates rebased to the
original bytes. Decorator expressions are parsed as source and never executed.

Rust uses the existing owned attachment grammar from symbol moves. Its exact
AST/entity binding includes outer attributes, outer documentation and comments
between those attachments and the selected declaration. Ordinary comments
before the attachment bundle, inner attributes and neighboring declarations
remain. Source-file, declaration-list and block item containers are admitted;
malformed syntax and dangling root/module attachments refuse. A leading UTF-8
BOM retains its original byte coordinates. No move intent or wire authority is
created to compute the deletion range.

Preparation creates the existing immutable derived snapshot and guarded plan.
Usage checks, force flags, permissions, approval, executed verification, stale
source preconditions and conditional recovery retain their separate contracts.
Syntax ownership does not prove that a declaration is unused or that the
result compiles. Verification runs only when explicitly requested and trusted.

Other language deletion boundaries retain their existing behavior. Separate
TypeScript member decorators, empty required Python bodies, general deletion
propagation, exhaustive reference coverage and IDE parity remain unsupported
by this finite contract.
File deletion has its separate [captured file usage contract](file-usage.md).
A separately bounded [ES function propagation policy](propagated-delete.md)
handles checked standalone call statements and newly unused private dependencies.
The explicit `symbol_delete_live` gate uses installed TypeScript provider and
compiler CLIs selected through `YOKE_TS_LSP` and `YOKE_TSC`, with no fallback to
an owned protocol fixture.
The separate `declaration_attachments_live` gate uses installed `YOKE_RUSTC` and
`YOKE_PYTHON` tools. It compiles owned Rust source and compiles/inspects the
Python stdlib AST without executing project modules, then checks approved
application and exact source recovery.
