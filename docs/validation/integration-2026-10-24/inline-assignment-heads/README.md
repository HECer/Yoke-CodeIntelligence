# Complete identifier assignment body expansion

The optional classic TypeScript/JavaScript body action now expands a complete
direct call on the right of a standalone plain identifier assignment:

```typescript
target = helper(argument);
```

The worker and independent Rust source reconstruction both derive the original
plain `=` assignment, require an identifier left side and the complete call on
its right, then require one whole expression statement. Parentheses around the
call and complete assignment remain original bytes. Assignment wrapper depth
has a shared 2,000-node bound. Existing call/AST/request/output/deadline limits
remain. The normal fixed block preserves unbraced `if/else` ownership. Original
target, operator, comments and statement suffix are source copies; only the
selected call becomes the existing checked result expression.

Resolving an identifier assignment reference adds no source evaluation ahead
of the call. Arguments still run once before the callee body; the actual write
still runs only after its completion. Throws retain the old target value.
Existing return/finally handling retains overridden or cancelled returns.
Properties, computed/destructuring targets, compound/logical assignments,
chained assignments and assignments used by a return/loop/another expression
remain unavailable because they need additional sequencing. This adds no general
expression lifting, application authority or claim of complete runtime equivalence.

`probe.cjs` assembles the ten unmodified worker components in actual
`projects.rs` order, using installed TypeScript5.8.3 and Node. A worker has the
existing 512 MiB Node heap, a 30 second diagnostic timeout and a 32 MB output
capture bound. This is trusted compiler execution, not an OS sandbox or a full
process-tree containment test. Only owned literal fixtures embedded in the probe
are transpiled/executed, with a one second VM timeout. Repository modules are
never executed as analyzed source.

The final **44 direct worker cases** are22 fixtures with removal/retention modes:

- Ten positive fixtures cover argument/write order including callee mutation of
  the target, unbraced branches, throwing calls, finally override/cancellation,
  contextually typed object returns, never returns, nested parentheses, Unicode/
  CRLF/comments and JavaScript JSDoc. All20 variants accept.
- Twelve adversarial fixtures cover properties, computed targets, effectful base
  expressions, compound/logical/chained/returned/nested assignments, loop heads,
  comma expressions, asserted targets and destructuring. All24 variants refuse
  with the original sequencing error after error-free source compilation.
- At parent `e125c30430cd70ce513d82fbb06f76492a1e9dde`, the same44 all refuse,
  including20 newly supported valid variants. This baseline is intentionally
  marked `matchesExpected=false`.

Every accepted current projection checks request/compiler/source identity,
before/after diagnostics, UTF-8 byte boundaries and result hash, actual copied
target bindings, unchanged target declaration types and equal owned runtime
observations. See `baseline.json`, `worker.json` and `checks.json`. These counts
are not native function counts or a native/platform acceptance receipt.

The separate22-case namespace regression also passes against this exact worker
assembly (twelve accepts, ten expected refusals). Typecheck, Node syntax, changed
three-file Rust formatting and npm260/0/3 existing skips pass. These scopes stay
separate; no combined native test count is inferred from them.

Three native test functions are added: positive compiler/Rust/runtime/source
checks, sequencing refusal without publication, and six coherently rehashed
forged writes which must fail specifically at independent source reconstruction.
The existing AST-field test gains exact assignment/caller/target checks and
wrapper-capacity boundaries. **None were compiled or run locally** because C:
remains below the established1.2GB native build admission. Older Yoke images are
not substituted. Native application, current platforms, live history/VSIX, full
R1/STATUS, main and release remain open.

Reproduce from the repository root, using fresh output directories:

```text
node --max-old-space-size=256 docs/validation/integration-2026-10-24/inline-assignment-heads/probe.cjs e125c30430cd70ce513d82fbb06f76492a1e9dde validation/assignment-baseline
node --max-old-space-size=256 docs/validation/integration-2026-10-24/inline-assignment-heads/probe.cjs worktree validation/assignment-current
```
