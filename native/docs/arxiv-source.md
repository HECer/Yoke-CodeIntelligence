# Received arXiv sources

The native `source-acquisition arxiv-capture` route receives one explicitly
selected arxiv.org abstract page or PDF. The host supplies a bare modern or
legacy ID, literal socket address and an absolute shared coordination file named
`arxiv-rate-v1.bin`. The generated HTTPS URL retains arxiv.org as its TLS name.
No search, bulk crawling, legacy export API, model or analyzed project execution
is enabled. Generic HTTP capture continues to refuse arxiv domains.

The independent grants `--allow-network`, `--allow-output` and
`--allow-rate-coordination` are required before config reads or filesystem/network
effects. A configuration document cannot grant permissions. Configuration fields:
`id`, `address`, `coordinationFile`, `representation` (`metadata` or `pdf`, default
`metadata`), `useSystemRoots` (default true), and optional
`rootCertificatesDerBase64`. The literal address must include the intended port;
the URL is generated from the ID and retains certificate name verification.
Static public roots are used by the existing bounded, no-proxy, no-redirect,
no-retry HTTP transport. There is no insecure TLS option or credential lookup.

```powershell
yoke source-acquisition arxiv-capture --config C:\host\arxiv.json --output C:\host\received.json --allow-network --allow-output --allow-rate-coordination
yoke source-acquisition read --input C:\host\received.json --arxiv-metadata
yoke source-acquisition arxiv-rate-status --coordination-file C:\shared\arxiv-rate-v1.bin
```

Use an address resolved and approved by the host. Source or imported archive
content never chooses endpoints, certificate trust or the coordination path.
The source archive retains exact response-body bytes and SHA256 bindings. It is
replayed without a workspace Store, network reacquisition or the original URL.
Frozen PDFs use the existing explicit `read --pdf-text` worker.

## Rate coordination and interrupted requests

[arXiv's API terms](https://info.arxiv.org/help/api/tou.html) require a single
connection and at least three seconds between requests across an operator's
machines. This connector conservatively applies the same interval to its
frontend metadata and PDF routes. The host must give all participating clients
the **same file** on a filesystem with coherent exclusive locks and consistent
UTC clocks. Other applications, different coordination files and clock skew
remain outside this local mechanism's guarantee.

The journal is a bounded two-slot checksum record. An exclusive OS lock remains
held from a durable pending admission through the owned request's termination.
Completion is durably recorded and starts a three-second cooldown. Errors and
timeouts do not silently reset the interval. A crash or failed completion leaves
a pending or corrupt journal; subsequent admissions refuse. Status reads never
create a missing file. Linked parents, linked/hardlinked leaves, malformed,
partial and inconsistent records refuse. Do not delete or replace a journal to
bypass its interval.

Recover only after the operator establishes that an earlier request is no longer
running. Read the current `journalHash` with `arxiv-rate-status`, then explicitly:

```powershell
yoke source-acquisition arxiv-rate-recover --coordination-file C:\shared\arxiv-rate-v1.bin --expected-hash <current-journalHash> --allow-rate-recovery
```

Recovery compares the exact expected journal hash under an exclusive lock and
completes a valid pending record at the current time. It starts a fresh cooldown
and never sends a request. It does not repair corrupt records. Stale hashes,
absent permissions, busy locks or regressed clocks refuse without replacing
the state. A lost stdout after successful recovery reports its committed journal
hash; a lost capture reply preserves the published archive and its identities.

## Metadata evidence and limits

`read --arxiv-metadata` interprets a finite lexical `<head>`/`<meta>` profile,
including the XML/numeric entity subset. It skips comments and raw script,
style and title content, excludes template/noscript metadata and stops at the
received head boundary. It never runs scripts, builds a browser DOM, retrieves
linked resources or interprets PDF positions as HTML evidence.

Every selected citation field is a separate observation with an occurrence ID
and exact original whole-tag byte range. Repeated author labels remain parallel
occurrences. Attribute decoding does not claim character-level source mapping.
Missing fields are absent, duplicate selected name/content attributes refuse,
and unsupported entities/profiles refuse. ID labels are compared only at their
base-ID level; this establishes neither publication truth nor a paper version.
URLs, title, date, abstract and author labels remain unverified received data.

The bounded policy admits at most 1,000,000 head-scan bytes, 10,000 tag events,
32,768 bytes per tag, 64 attributes per tag, 64 exclusion-stack entries, 1,024
selected occurrences and 2,000,000 decoded field bytes. Existing body/archive,
deadline, cancellation and output quotas still apply. `complete` and
`sourceAuthority` remain false. This is not a canonical code snapshot, author
identity service, article/paper semantic graph, citation database or full
upstream parity certification.

The received public page or PDF can contain copyrighted text. Validation retains
public payloads locally with published hashes/receipts; committed replay fixtures
contain only project-owned test data.
