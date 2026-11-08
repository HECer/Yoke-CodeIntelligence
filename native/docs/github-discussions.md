# Closed PR discussions and captured-source digests

`github-discussions capture` performs explicitly authorized GitHub REST GETs. It
does not open a source Store, run project code, call a model, publish a comment or
send data to a service. The network profile and request are separate absolute
host files. Both CLI permission flags and the corresponding profile permissions
are required before credential lookup or network access.

```json
{
  "read": {
    "allowNetwork": true,
    "allowedRepos": ["owner/repo"],
    "maxPages": 5,
    "maxItems": 100,
    "maxRequests": 40
  },
  "allowDiscussionText": true
}
```

```json
{
  "schemaVersion": 1,
  "repository": "owner/repo",
  "repositoryId": 123456,
  "pullRequests": [42]
}
```

The repository ID above is a placeholder. Use the actual provider ID and the
canonical lowercase repository name. An empty `pullRequests` array selects the
bounded closed-PR listing ordered by update time. Explicit numbers must be
unique and currently closed. Redirects, proxies and automatic retries remain
disabled; a renamed repository must use its current canonical name. Credentials
remain optional for public reads. A `read.credentialRef` selects the existing
host OS credential reader, never a token supplied in a request or repository.

```sh
yoke --max-bytes 8000000 github-discussions capture \
  --profile /host/discussion-profile.json --request /host/discussion-request.json \
  --allow-network --allow-discussion-text
yoke --max-bytes 8000000 github-discussions inspect --capture /host/capture.json
```

Capture emits a closed, versioned JSON packet to stdout. Redirection or saving
that output is an explicit host action. Description, issue comments, reviews and
review comments retain exact decoded UTF-8 text, text hashes, reported times,
commit IDs, reply/review IDs and review diff coordinates. A JSON `null` body is
known absence; a missing required body field is a protocol error. Replies remain
directional observations, including references to unavailable parents. Provider
coordinates are not converted into verified source-byte anchors.

Repository numeric identity and PR base scope are checked. Closed state, PR ID,
head/base hashes, title, body, closure/update times and merge state are rechecked
after acquiring the channels. Each HTTP response has a bounded hash/byte receipt.
These are observations across requests, **not an atomic provider snapshot**.
Coverage means provider-reported pagination under the selected read visibility;
it does not establish undeleted history or invisible pending reviews.

The existing per-request/page/item/raw-response limits are shared across the
operation. Additional hard limits are 100 selected PR numbers, 1,000 observations,
4 MB of aggregated text and 8 MB of encoded output. A final metadata reread is
reserved while paging each PR. PRs not started within the request budget appear
in `unacquiredPulls`; partial channels and discovery keep incomplete coverage.
Explicit selections that cannot fit their minimum requests refuse before
network. Capacity failures never silently clip a text or observation. Existing
high-confidence credential patterns refuse the operation without echoing text;
this is not a general DLP guarantee.

Loading a packet rechecks its closed shape, canonical content hash, numeric/text
budgets, endpoint/origin/query receipts, page counts, selected/unacquired PRs,
observation identities, text hashes and coverage claims. Its hash binds content;
it does not authenticate an author or provider. Programmatic loopback fixtures
are tagged and rejected by digest composition unless the host explicitly permits
fixture discussions. Imported captures remain unsigned host observations even
when their transport field reports a prior HTTPS acquisition.

## Offline source-bound composition

The existing digest command remains offline and requires its original App grant
and captured source request. Add a capture explicitly:

```sh
yoke --root /project --max-bytes 4000000 github-app --profile /host/app-profile.json \
  digest --allow-digest --allow-discussions --discussion-capture /host/capture.json \
  preview --request /host/digest-request.json
```

The same options work with `history`, including explicit selected existing
captures. No automatic source acquisition is added. The current App grant and
repository ID/name must match the imported capture. Native source replay retains
its immutable bytes, exact anchors, source and metadata capacities, SQL policy
restoration and existing Rust export-syntax limitations.

Each PR reports whether its claimed head matches a locally verified selected
capture, occurs only in verified local history without a selected source capture,
or is unavailable. None establishes remote repository ownership, source equality
of a discussion coordinate, compiler binding or runtime dispatch. Provider text
cannot select a filesystem path, executable, model, credential or permission.

## Reviewable service payload preview

Add `--handoff-preview` to produce a bounded content-addressed artifact containing
verified native export headers, source/capture identities and discussion metadata.
It excludes discussion bodies, titles, diff hunks, author strings and commit
messages. The native header projection excludes top-level function bodies and
constant/static initializers. Symbol omissions, unsupported files and visibility
limits remain explicit. This is structured exclusion of those bodies, not a
claim that headers disclose no source information or that arbitrary secrets have
been scrubbed.

The artifact declares `reviewRequired:true` and `serviceSend:"unavailable"`.
Generating it performs no send, durable approval or publication. The separate
[service workflow](digest-service.md) regenerates this projection and binds
explicit approval to the exact submission bytes and host-selected endpoint.
It implements a durable intent/receipt lifecycle and GET-only recovery for the
independently owned receiver contract. Configured production acceptance, full
upstream/proprietary brain behavior and live App deployment remain uncertified.

The acquisition follows the primary GitHub
[issue-comment](https://docs.github.com/en/rest/issues/comments),
[review](https://docs.github.com/en/rest/pulls/reviews) and
[review-comment](https://docs.github.com/en/rest/pulls/comments) contracts with
the existing pinned API version. Live validation records only metadata, hashes
and assertions; fetched public discussion texts are not published as evidence.
