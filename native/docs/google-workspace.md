# Explicit Google Workspace export

This adapter selects one `.gdoc`, `.gsheet` or `.gslides` shortcut by original
SHA-256 and Google file ID. Docs request Markdown, Slides request plain text,
and Sheets request XLSX. Shortcut JSON is data: it cannot select a program,
endpoint, account configuration, permission, model, or output path.

Capture, immutable archive and replay pass owned Windows unit/CLI/process
contracts. The installed gws 0.22.5 executable has been checked with `--version`,
root help, the public Drive export schema and an actual empty-state ADC-fallback
refusal. An authenticated Google export has not yet been validated. A resource key in a
shortcut currently returns `google_workspace_resource_key_transport_unavailable`.
It is never silently discarded. This is not complete R7 acceptance.

## Capture

Select absolute paths for a trusted, preinstalled gws executable and its existing
configuration directory, both outside the analyzed source tree. The executable
hash is an exact host precondition; the version must be `0.22.5`. Account labels
inside a shortcut remain unverified declarations. No login, token discovery or
automatic account selection is performed by the native parent.

Example host configuration (replace both hashes and absolute paths):

```json
{
  "shortcutPath": "notes/design.gdoc",
  "expectedShortcutSha256": "<original shortcut SHA-256>",
  "expectedFileId": "<selected file ID>",
  "client": {
    "executable": "C:\\host-tools\\gws.exe",
    "executableSha256": "<selected executable SHA-256>",
    "version": "0.22.5",
    "configurationDirectory": "C:\\host-state\\gws"
  }
}
```

```sh
yoke source-acquisition google-capture --config /absolute/capture.json \
  --source-root /absolute/project --output /absolute/new-export.json \
  --allow-shortcut-read --allow-network --allow-exporter-execution \
  --allow-client-state --allow-output
```

Every permission is host-only and required before reading configuration. Existing
output files, directories and links refuse before the client starts. Output
capacity is admitted before execution, then publication uses a fresh file without
replacement. The shortcut must still match after the client finishes. A stdout
delivery failure after publication reports the committed archive identity.

The client runs in an owned temporary directory with an empty `.env`, fixed
arguments and the selected configuration directory. The parent removes inherited
Google credential overrides, proxy, logging and Model Armor settings. The normal
process host environment allowlist still applies. No analyzed source or project
configuration is executed. There is one version check and one export invocation,
with one shared deadline and bounded stdout/stderr. There is no native automatic
retry or redirect machinery. gws itself, its selected configuration and account
store are trusted dependencies, with their own discovery and HTTP behavior.

gws also has a well-known gcloud Application Default Credentials fallback outside
its selected configuration. The adapter blocks that fallback with an explicit
nonexistent ADC path in the owned job directory, so only selected gws state is
used. It does not inspect or copy the user's unrelated gcloud credentials.

This is not an OS sandbox, network attestation or filesystem quota. The exported
file is bounded when admitted; the parent does not impose a live disk quota on
the trusted client. Unknown process cleanup retains the owned temporary directory
and returns failure. Client diagnostics are not copied into native errors.

Optional `--limits /absolute/limits.json` narrows `maxBodyBytes` (at most
10,000,000) and `maxArchiveBytes` (at most 16,000,000). Both must be positive.
The worst-case base64 envelope must fit before execution. Body, archive and CLI
output budgets are separate.

## Offline replay

```sh
yoke source-acquisition google-read --input /absolute/new-export.json
yoke source-acquisition google-read --input /absolute/new-export.json \
  --start-byte 0 --end-byte 64
yoke source-acquisition google-read --input /absolute/sheet-export.json --office-text
```

Replay opens no Store, account or client and makes no network/model call. The
closed archive schema checks MIME/kind, declared length, canonical base64, exact
body SHA-256 and domain-separated identity. Imported client, shortcut and account
claims establish no remote identity or execution authority, even if coherently
resealed. The archive stores the shortcut hash and safe metadata, not its raw
pointer bytes, resource key or account label.

Ranges refer to exact received export bytes and return base64 without lossy UTF-8
replacement. UTF-8 is included only when the complete slice decodes; each read is
at most 65,536 bytes. XLSX bytes remain their original container source space.
Separate `--office-text` inspection invokes the bounded native Open XML parser,
preserving XML/cached-cell provenance and never evaluating formulas, following
external relationships or claiming rendered layout. No export is automatically
indexed or semantically interpreted.

Public interface references: [gws 0.22.5](https://github.com/googleworkspace/cli/tree/v0.22.5)
and [Drive export](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/export).
gws describes itself as not officially supported by Google. No upstream application
code is vendored by this adapter.
