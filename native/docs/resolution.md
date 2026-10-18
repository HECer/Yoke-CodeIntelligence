# Syntax-local source resolution

The Rust parser records lexical scopes, declarations, imported names, aliases, explicit parameter/property types and syntax-local constructor assignments. Graph assembly uses that metadata against the captured source inventory. It never executes analyzed code, reads external dependency source implicitly or invokes an upstream application.

Calls and non-call references retain exact UTF-8 ranges and the immutable file hash. A call anchor remains the full call expression; an ordinary reference anchors its identifier or complete member/path expression. A module import is separate from a symbol reference. Class inheritance and interface implementation have distinct `extends` and `implements` observations where the source syntax distinguishes them.

A resolved structural target is labeled `heuristic`, including a target suggested by an annotation or constructor. This is a syntax-local candidate, not a compiler binding. A union receiver can produce parallel observations with different targets sharing the same source evidence. Unknown receivers, missing modules, overwritten bindings and ambiguous unqualified names remain unresolved rather than selecting an arbitrary method.

Nested functions and blocks use their own declarations. Parameters and local bindings can prevent a same-named global function from being selected. A reassignment invalidates the earlier receiver candidate; a conditional assignment conservatively prevents a definite receiver inference after that branch. This is conservative syntax analysis, not whole-program control-flow or heap analysis.

Imports are resolved only to files already in the selected snapshot, with compatible language families. Captured TS/JS modules, Python modules and packages, and Rust module/use paths retain their declared aliases. Multiple captured source candidates remain separate. Framework routing, dynamic imports, generated modules, package installation, arbitrary compiler configuration and complete dependency/visibility semantics are not inferred.

Method ownership retains its lexical namespace/class. Rust implementation declarations provide method owners without becoming replacements for the declared struct/type. Explicit interface members and implementation targets remain candidates; dynamic virtual dispatch and every possible runtime receiver are not proven.

Entity signatures contain the source header without the executable body, including multiline parameter and return declarations. Headers longer than 4,096 UTF-8 bytes have an explicit warning diagnostic and a bounded prefix. Source reads remain authoritative for the complete declaration. A file API must disclose that clipping rather than presenting the prefix as a complete signature.

Extraction enforces the original shared deadline, one million AST nodes, 100,000 scopes/bindings and 200,000 use records per file. Alias traversal and inheritance are bounded; lookup work and source capture retain their separate request budgets. A work-budget or deadline failure is an error, not an empty complete result. Shared inheritance paths are memoized to prevent repeated traversal from amplifying work.

This contract covers the implemented syntax-local cases. Real compiler and language-server operations are separate host-selected paths, with their own capability and source-validation contracts. Grammar coverage alone does not establish complete handling of a language's constructs.
