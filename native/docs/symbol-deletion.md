# Symbol deletion boundaries

`deleteSymbol` preparation derives a separate declaration deletion range for
TypeScript, TSX, JavaScript and JSX. Structural entities and provider usage
anchors keep their original byte coordinates. This also governs the final
preparation step of symbol-selected `safeDelete` and its facade aliases.

The selected entity must bind one exact, error-free syntax node: declaration
range, name range, name text, kind, path and language all agree with captured
source. The existing captured-source policy supplies hash-checked bytes. The
parser and bounded traversal observe the request's original deadline and host
cancellation. Input is limited to 4,000,000 bytes and 200,000 visited AST nodes;
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

Preparation creates the existing immutable derived snapshot and guarded plan.
Usage checks, force flags, permissions, approval, executed verification, stale
source preconditions and conditional recovery retain their separate contracts.
Syntax ownership does not prove that a declaration is unused or that the
result compiles. Verification runs only when explicitly requested and trusted.

Other language deletion boundaries retain their existing behavior; this
checkpoint does not establish Python decorator or Rust attribute ownership,
general deletion propagation, exhaustive reference coverage or IDE parity.
File deletion has its separate [captured file usage contract](file-usage.md).
The explicit `symbol_delete_live` gate uses installed TypeScript provider and
compiler CLIs selected through `YOKE_TS_LSP` and `YOKE_TSC`, with no fallback to
an owned protocol fixture.
