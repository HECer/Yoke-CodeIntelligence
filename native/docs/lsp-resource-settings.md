# Source-bound LSP resource settings

Optional resourceSettings is a map from exact captured file paths to settings
objects. It is accepted by HostConfiguration and catalogue HostRegistration,
and by each trusted project/backend languageServerSettings entry. Pinned YAML
ls_specific_settings uses the same backend entry shape. For CLI
--language-server-config entries it belongs inside configuration.

Example client-side data selection (exampleSection is illustrative vendor data):

    {
      "settings": {"exampleSection": {"base": true, "style": "base"}},
      "resourceSettings": {
        "src/example.rs": {"exampleSection": {"style": "selected-file"}}
      }
    }

The file overlay replaces exampleSection as one top-level key; nested objects
are not recursively merged. A project overlay first merges each named file's
object with that file's ORIGINAL registered overlay. Each resolved file overlay
then merges with the selected workspace settings. Existing whole-backend
project-local/project/global precedence stays unchanged; another project's
effective configuration never becomes the starting point.

## Admission and authority

Paths must be canonical portable relative, non-protected file paths, at most
4,096 bytes. Every selected file must exist in the immutable captured Snapshot
and remain outside passive private-tree omissions. No host/original-source
lookup, glob, directory inheritance or automatic file discovery is added.
Resource data cannot register a provider, change its launch program, grant
source execution or writes, select an outside root, bypass source freshness or
replace an approval/recovery requirement.

At most 100 file entries and 128,000 serialized map bytes are admitted.
Each raw overlay and its effective workspace merge follow the existing
64,000-byte/32-value-level/4,096-node project-setting limits. Passive Rust's
mandatory false execution settings are added to each resolved file object and
its final capacity is rechecked before process startup. Existing overall
host-profile and protocol budgets remain independent. These are bounded
application checks, not a hard OS memory or execution sandbox.

Passive checks inspect initialization, workspace and every effective file object.
Direct and rust-analyzer-namespaced Rust settings are both checked; neither can
mask the other. Existing blocked compiler environment selectors are compared
with ASCII case insensitivity. Explicit source-execution permission remains a
separate host field. Generic/vendor data stays distinct from validated runtime
behavior; no extra vendor aliases or runtime versions are certified.

## Configuration callbacks

An explicit map, including an empty map, activates captured resource resolution.
The copied tree's canonical root URI and absent/null scopeUri select workspace
settings. An exact captured file selects its effective overlay, or workspace
settings when no file override exists. A valid but unavailable/foreign/protected/
omitted/non-file scope returns null for that item; no real host file is opened.
Relative file membership is exact; no Unicode normalization or expanding
case-fold identity is introduced.

Sections retain the existing dotted-key projection and response order.
Non-object items, invalid non-null scope types and overlong scope strings fail.
At most 64 items, 8,192 scope bytes, 1,000 section bytes and the existing
4,000,000-byte callback reply limit apply. These semantics follow the
[LSP configuration request](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/workspace/configuration.md).

Omitted/null host fields retain the preceding no-map format. Omitted/null
project overlays retain original registered defaults. Valid absent/null wire
replies, default profiles and named bindings remain unchanged. Selected maps
bind the existing provider/session fingerprint, and changed named policy refuses.
HTTP authentication/current principal grants/revocation still precede use.
Configuration remains fixed for one provider run; there is no dynamic editor
configuration import or running-session edit mechanism.

Owned Node protocol, actual CLI/default-row and authenticated TCP tests qualify
this mechanism. They are not vendor runtime or semantic-quality certification.
The actual current explicit-runtime VS Code host test passes separately; a
canonical bundled VSIX, all 77 runtimes and release acceptance remain open.
See ../../docs/validation/integration-2026-10-23/lsp-resource-settings/README.md.
