# Lossless host configuration edits

`host_config` is a pure byte-edit planner. It receives explicitly supplied bytes
and returns bounded edits and hashes; it does not discover files, read a home
directory, write configuration or grant installation authority.

The generic `AGENTS.md` profile owns one block delimited by
`<!-- yoke:managed:generic-agents-v1:start -->` and its corresponding `:end`
marker. Creating a block refuses any pre-existing reserved Yoke marker. Updating
or removing it requires a prior owned-fragment receipt, supplied only after the
installer validates its durable receipt. The exact fragment and its hash must
still match. A marker alone is not a receipt, and serialized patch data is not a
write capability.

Unowned bytes remain unchanged, including UTF-8 BOM, Unicode, comments and mixed
line endings. The managed body uses the first existing line ending, or LF for an
empty file. A missing final newline is added as part of the owned fragment, so
removal restores the original bytes. A human prefix or appendix outside the
owned fragment survives later updates/removal. An edited managed block, inline,
duplicate or nested reserved marker requires manual reconciliation.

Patch application rechecks the entire input hash, every replaced byte range and
the resulting hash. The input/output limit is 1 MiB; the supplied body limit is
64 KiB. Invalid UTF-8, NUL bytes and expired caller deadlines fail before a usable
output is returned. Errors do not quote configuration content.

The installer stores only owned and inverse fragments. The planner intentionally
does not include the complete original or resulting configuration in a receipt.
This generic capability does not establish compatibility with a specific
Codex, Claude, Cursor or other client configuration schema.

## JSON and JSONC named objects

`jsonc_patch` edits one named JSON object under an explicitly selected, existing
parent object. It preserves all neighboring bytes, including comments, trailing
commas, whitespace and BOM. The new value must be strict JSON with unique keys;
the complete configuration is checked for duplicate *decoded* keys, including
escaped aliases. Array entries and client-specific hook matching are outside
this generic named-object contract. Missing/non-object parent paths fail rather
than guessing a client schema or creating a new hierarchy.

The receipt owns the inserted key/value and any separators added with it. If a
human later appends another member, removal preserves that member when the
recorded separators still produce valid JSONC. Removal fails if it would need
to delete or move an unrecorded human comma. Updating an intact owned member
does not rewrite sibling values or comments. Parsing is limited to 64 levels
and 16,384 values; malformed strings/comments and ambiguous keys fail closed.

## TOML managed tables

`toml_patch` owns one appended table with an explicit path of bare key
components. The supplied body contains flat keys without separate table
headers or dotted child keys. Existing target tables, quoted aliases, dotted
assignments and descendants conflict with a new installation. Other existing
tables remain byte-identical. Standard TOML parsing checks the complete
document, while a lexical scan distinguishes real headers from brackets inside
multiline strings, arrays and comments.

Human comments or subsequent explicit table headers may follow the managed
table and survive its removal. Human keys appended under the managed table
require manual reconciliation: deleting its header would reparent those keys,
so both update and removal refuse the operation. Updates/removal require the
same receipt selector and intact owned bytes. Removal also refuses when deleting
an owned separator would join an unterminated preceding human line with its
appendix; valid syntax alone cannot prove that a following header kept its
meaning. An appendix beginning with its own LF/CRLF remains removable, and an
update retains the owned separator. TOML documents are limited to 16,384 lines.
Table editing is a generic
capability and does not certify any branded client's TOML schema.

## Strict JSON MCP entries

`json_mcp_patch` is a separate versioned format for a named `mcpServers` member.
It admits strict unique-key JSON with an optional BOM, creates absent containers
or documents, and records exact owned entry and created-shell boundaries.
Ownership is scoped to the decoded member path, so identical bytes in an unrelated
nested value do not conflict. Removal preserves foreign members and whitespace,
removing only its own entry and a required comma. Created shells disappear only
while their exact owned wrapping bytes remain unchanged. Modified entries and
unowned existing names refuse. Serialized ownership/patches never grant writes.
See the [JSON host contract](../../docs/native-json-hosts.md) for bounds, physical
bindings, Windows recovery, client evidence and remaining runtime qualifications.
