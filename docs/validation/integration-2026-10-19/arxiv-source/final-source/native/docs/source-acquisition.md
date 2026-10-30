# Explicit received HTTP sources

`source-acquisition capture` performs one host-authorized public GET to one
explicit literal socket address, preserving the received representation body in
a create-only immutable archive. The URL supplies the HTTP/TLS name; the address
must have the same port and supplies the connection target without DNS. Literal
URL IPs must match it. Only HTTPS and separately permitted literal-loopback HTTP
are available. There are no credentials, cookies, proxy, redirects, retries,
embedded-resource requests or project execution. No MCP query can acquire data.

```json
{
  "url": "https://example.com/",
  "address": "<host-selected literal IP>:443",
  "format": "html",
  "useSystemRoots": true,
  "rootCertificatesDerBase64": []
}
```

Formats are `text`, `html` and `pdf`. The profile is closed: network/output
permission cannot come from configuration, source content or archive claims.
`capture --config /absolute/config.json --output /absolute/fresh.json
--allow-network --allow-output` admits destination and delivery metadata before
network activity. Loopback HTTP additionally needs `--allow-loopback-http`.
Existing files/links are preserved. Lost stdout after publication retains the
archive and reports its source/archive identities. Source/provider/root flags
are unavailable and no source Store is opened.

TLS copies public OS trust anchors when enabled and combines explicitly supplied
DER roots. `tls_certs_only` uses offline WebPKI verification, including hostname
checks, instead of the platform verifier's possible AIA/OCSP activity. Root load
errors fail explicitly. Roots and library parsing are OS/library boundaries;
no hard bound on every library allocation is claimed.

Default hard limits are 10 MB accepted body, 16 MB archive, 64 HTTP headers and
16 KB admitted normalized header fields. A closed optional `--limits` file can
lower body/archive limits. Status must be200; missing/duplicate content type,
content encoding, redirects, transport truncation and overflow fail explicitly.
Chunk transfer framing is decoded by the HTTP library. Evidence addresses the
received representation body, not original wire headers/framing. Selected
content-type/entity-tag/last-modified fields are unverified normalized data.

`read --input /absolute/archive.json` inspects frozen metadata. Explicit
`--start-byte N --end-byte M` returns up to65536 exact bytes as base64, plus UTF-8
only if the entire slice is valid. Split Unicode sequences are never replaced.
Body hashes and typed metadata are replayed under independent input/output and
deadline caps. Imported capture claims never grant source, server, network,
filesystem or execution authority; `complete` remains false. Replay counters
describe received-byte processing, separate from the authorized live capture.

HTML remains raw data. No browser DOM/article/tweet/video/semantic content is
inferred and scripts/links/instructions are not followed. Plain text is literal
UTF-8. For PDF, explicit `read --pdf-text` runs only the installed native binary's
existing bounded PDF parser on frozen bytes in an owned temporary directory,
without an original asset path or Store. It retains the parser's direct-content,
font, resource, OCR and coverage limits from [media](media.md); it is not a
general OS sandbox or complete PDF interpretation.

arXiv domain requests use the [dedicated coordinated connector](arxiv-source.md);
they are refused by this generic transport. Google shortcuts, authenticated services,
social/video-specific routes and graph-database upsert are separate
capabilities. A public endpoint timeout is unavailable live evidence, never an
empty complete source. Actual backend/platform evidence is recorded separately.

Primary HTTP client contract: [reqwest0.13.5 ClientBuilder](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html).
