# Current Serena Session parameters

At Serena revision `ede329aaf0c824bea75455a2926f01420d7aa620`, the public
[activate_project declaration](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/tools/config_tools.py)
and [serena_repl declaration](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/tools/repl_tools.py)
use `session` for the caller's Session identifier. The preceding pinned contract
used `session_id`. The current
[metadata builder](https://github.com/oraios/serena/blob/ede329aaf0c824bea75455a2926f01420d7aa620/src/serena/tools/tools_base.py)
exposes these method parameters to MCP clients. These are public-interface facts;
no Serena application implementation is copied into Yoke.

The native adapter accepts either spelling for **these two tools only**:

```json
{"project":"default","session":"owned-session"}
```

```json
{"session":"owned-session","code":"40 + 2"}
```

The first object is an `activate_project` argument; the second is a
`serena_repl` argument. A legacy `session_id` object remains accepted. The Session
identifier must exactly match the already host-owned native Session. It cannot
create or select another Session, register an arbitrary project, select a Python
executable, supply runtime authority or bypass activation policy.

Exactly one spelling is required. Supplying both refuses even when their values
are identical. Missing, null, nonstring, Unicode-confusable names and unknown
fields retain strict refusal. The original raw-request and canonical-argument
4,000,000-byte budgets both apply. Only a correctly typed current key is
normalized into the legacy argument shape; the existing ownership check and
execution paths remain unchanged. Tool exclusion, current Session state and
deadline checks still precede dispatch. The generic argument validator is not
expanded to accept arbitrary unions or aliases.

Public discovery advertises both string properties, marks `session_id` deprecated,
and uses `oneOf` to require exactly one. The original required `project` or `code`
field and closed object schema remain. The compiled data overlay
[`compatibility_current_sessions.json`](../src/compatibility_current_sessions.json)
pins the two upstream source blobs and only these two schemas. The historical
53-tool `compatibility_contracts.json` stays byte-exact. Tool order, descriptions
and the other 51 schemas remain unchanged. The two current discovery schemas are
new observable output; this is not a claim of whole-catalog byte identity.

Without an explicitly configured host Python runtime, `serena_repl` still refuses
execution after checking the owned Session. With that separately trusted runtime,
both spellings use the same persistent cells and existing source/write/shell
permissions. The alias itself grants no execution or additional source access.

Two independent inert public-data tests observed the missing current schema
before implementation, then pass with the overlay. They evaluate only its closed
object/string/required/oneOf subset and never execute native or upstream code.
Three native unit tests are authored for exact legacy normalization, invalid and
ambiguous inputs, unrelated-tool refusal and byte budgets. Four integration tests
are authored for discovery preservation, host ownership/no-runtime refusal, actual
Serena stdio routing and persistent Python state across both spellings with
foreign/ambiguous calls rejected. Their Unicode/CRLF source fixtures must stay
unchanged.

Native compilation and these seven native tests are **unrun locally** and require
source-bound hosted qualification. This interface correction does not certify
the broader current Serena implementation, backend runtimes, combined feature or
platform behavior, main or release acceptance. The original 353-entry assessment
ledger is unchanged.
