# Explicit pinned prompt catalogs

The native host can select YAML prompt catalogs, render a named prompt, or use
catalog names in the existing context/mode prompt and project/local initial_prompt
projection. No directories, default prompt texts or runtime variables are selected
automatically. The independent core remains MIT; no upstream application or
prompt text is copied.

A catalog contains exactly one mapping:

```yaml
prompts:
  heading: 'Evidence for {{ subject }}'
  system_prompt: |
    {% include 'heading' %}
    Keep original source evidence.
```

Names and prompt values must be strings. Names are case-sensitive opaque template
identifiers, at most 200 UTF-8 bytes. Unicode spelling is preserved. Relative
slash-separated names work; empty/dot/dot-dot segments, backslashes, colon,
controls and the reserved [native prompt] name refuse. Names do not select files.
Aliases, anchors, tags, merge/duplicate keys, multiple documents, extra root keys,
non-string values, NUL and invalid UTF-8 refuse through the existing bounded YAML
parser. An explicit prompts: {} is a valid observed empty catalog.

## Pinned selection and collisions

An existing policy selection may include this templateContext:

```json
{
  "variables": {"subject": "owned"},
  "templates": {"shared": "inline base"},
  "catalogs": [
    {"path": "C:/host/prompts.yml", "sha256": "<original byte SHA256>"}
  ],
  "collisionPolicy": "error"
}
```

Plan/export require original-byte pins for every catalog, including shadowed
files. Pass --allow-template-evaluation and the existing configuration-read and
policy-projection grants; export also needs --allow-output. The grant is checked
before catalog access. Paths must be explicit absolute local host paths. Existing
ancestor/reparse, no-follow, hardlink, guarded-read and before-publication byte
checks apply. Actual held file identities additionally reject repeated catalog
handles, including path aliases. Guards remain held through delivery.

The default error policy refuses repeated prompt names. Explicit last_wins
applies inline definitions first, followed by catalog files in supplied order.
Within each file names are sorted exactly. Each declaration spends budget even
when shadowed. promptCatalogResolution records each raw source hash/size, decoded
prompt hash/size, origin, replacement links and effective status; no declaration
is silently removed. Its canonical hash also binds rendering evidence. Decoded
YAML byte spans are unavailable and are explicitly labelled false; decoded hashes
are not claimed to be exact raw-source offsets. Rendered guidance creates no
runtime, project registration, tool, write or policy authority.

## Host-selected discovery

session-config discover additionally accepts a catalogs array of path/optional
sha256 candidates and directories with kind: prompt_templates. Catalog selectors
require --allow-template-evaluation before directory/payload access. They can
create an otherwise empty templateContext explicitly. Existing pinned context
catalogs come first, then explicit discovery catalogs, then selected directories
in input order with exact sorted .yml/.yaml filenames. Catalog filenames do not
select prompt names, so Unicode filenames are supported.

Every directory/candidate/count/target check precedes YAML payload reads. Scans
are flat, bounded to 32 directories and 1024 entries each, including non-YAML
entries. The ready selection retains ordered original-byte pins and must fit the
ordinary 128000-byte selector. Directory membership and source bytes are rechecked
before stdout; empty selected roots are observed empty, missing roots refuse.
Discovery never searches home/project/default locations. Replay still needs its
independent host grants.

## Named rendering and create-only text output

The context JSON file contains the templateContext object itself, without a
hostProfile or policy selection. With command wrapping shown:

```text
yoke --max-bytes 2000000 session-config render-prompt
  --context C:/host/context.json --name system_prompt
  --allow-configuration-read --allow-template-evaluation
```

Optional --output C:/host/new-prompt.txt --allow-output creates plain rendered
UTF-8 at a fresh guarded destination. Existing files are preserved; all parse,
render, source-freshness and reply-capacity checks precede creation. A publication
followed by stdout failure reports the committed text hash. The reply binds the
original context JSON, selected entry, resolved data, original decoded prompt,
rendered bytes and catalog provenance. No Store, model, network or analyzed-source
execution is involved. The template worker itself is an explicitly selected native
host process. Exported session profiles contain literal strings and do not rerender.

## Budgets and compatibility

There are at most 32 catalog files, 512 declarations including replacements, and
256 effective names. Each source is at most 65536 bytes. Policy and catalog raw
bytes together share the existing 2000000-byte source ceiling. Host context and
selection JSON remain bounded to 128000 bytes; inline definitions remain limited
to 32. Resolved variables retain a 128000-byte JSON bound. New catalog/named
requests use protocol 2 with at most 8000000 JSON bytes; this does not raise the
preceding inline protocol 1 limits of 128000-byte context, 32 names and
256000-byte request. Old literal/default and inline replies retain their bytes.

The same MiniJinja 3.0.3 inert data conversion, strict missing values, no filesystem
loader/environment/application interfaces, 100000 fuel, recursion 64, 8192-byte
output, inherited/10s deadline and checked 256MiB parent/self OS memory ceilings
apply. Worker protocol 2 binds a named entry to its actual selected decoded text.
Request JSON cannot supply filesystem catalogs or host callbacks. OS limits are
resource controls and do not constitute an account/network/filesystem sandbox.

Automatically derived runtime/tool variables, default/upstream catalogs, exact
Python-Jinja conformance, external layouts/extensions/clients and broad upstream
parity remain separate. All broad complete fields remain false. Current Windows
scope and remaining release/runtime limitations are recorded in the linked
qualification evidence; Unix execution is not certified by Windows tests.

Qualified code cb901cec: 742 passes, seven existing ignores, no failures or filtered
functions in 20 whole Rust groups, including 20 added functions. The 13 actual
CLI parents are outside those Rust totals. See [source-bound qualification](../../docs/validation/integration-2026-10-23/prompt-catalogs/README.md).
