# YAML expansion admission

Configuration graphs and skill frontmatter share `config_graph::bounded_yaml`.
The event pass runs before `YamlLoader` materializes aliases. It admits at most
20,000 expanded nodes, nesting depth 64, and 8 MiB of expanded decoded scalar
UTF-8 bytes across the complete YAML stream. The existing limit of 100 parsed
documents also remains in force.

Scalar byte accounting includes mapping keys, alias subtree contents, and the
additional copies retained for anchored scalars and anchored containers. Nested
container aliases therefore carry their complete scalar cost. The accounting
uses checked arithmetic and does not reset at document boundaries. Numeric and
boolean scalar spellings are conservatively charged even when the loader stores
them as typed values. Parser and container bookkeeping have separate overhead;
the scalar limit is not a promise that total process allocations stay below
8 MiB.

Small aliases and Unicode scalars remain supported. Accepted documents retain
the original parser results, source bindings, and graph identities; no source or
derived metadata is rewritten. Exceeding the new scalar budget fails before
loader cloning and before derived graph publication.

`tests/yaml_admission_review.rs` measures actual allocation requests around the
public configuration extraction call. Bounded fixtures cover scalar aliases,
nested container aliases, independent document totals, and retained nested
anchor copies. A positive fixture checks Unicode, anchor reuse across independent
documents, and deterministic graph output.
