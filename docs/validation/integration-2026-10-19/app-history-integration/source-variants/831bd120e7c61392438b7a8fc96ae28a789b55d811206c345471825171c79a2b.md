# App-bound persistent history execution

`github_app::worker::history` joins an authenticated durable worker completion to
the existing bounded local Git history job. An explicitly admitted worker run can
complete source review, bind that completion, and execute bounded history batches
without a separate manual handoff. Existing worker runs retain their original
behavior unless this optional execution is selected. No background service,
production installation, model call, project-code execution or publication starts.

## Host-selected worker execution

```sh
yoke --root /project --allow-writes --max-bytes 4000000 github-app --profile /host/app.json --allow-webhooks --allow-github worker run --request /host/run.json --auth-profile /host/auth.json --history-schedule /host/history.json --allow-app-history --allow-history-jobs --allow-history-capture --trust-history-git
```

The ordinary worker request and App/authentication profiles retain their existing
contracts. Its head must already be a checked Git capture, matching the completed
worker's observed head commit. An ordinary disk snapshot does not grant this
history capability. `--history-git` can select an explicit trusted Git executable.
The separate history grants and output budget are checked before files/keys;
all request/profile/schedule shapes are checked before credential loading.
The source receipt is checked before worker HTTP. Actual worker authorization,
source replay and provider checks remain in force.

The closed schedule contains `schemaVersion:1`, `expectedQueueRevision`,
`historyRevision`, `maxBatches` (1..128), and a complete original `plan` using
the [history-job Plan](digest-history-jobs.md). For example, the default plan is:

```json
{
  "schemaVersion": 1,
  "expectedQueueRevision": 2,
  "historyRevision": 0,
  "maxBatches": 8,
  "plan": {
    "traversal": {"objects": 16000, "rawBytes": 16000000, "parentIds": 64000, "frontier": 4000},
    "maxCaptures": 16000,
    "maxCandidateFiles": 1000000,
    "maxSourceBytes": 1000000000,
    "maxCommitBytes": 256000000,
    "maxTreeBytes": 256000000,
    "maxTreeEntries": 1000000
  }
}
```

Use the actual observed queue/job revisions, rather than assuming these example
numbers. Saved JSON cannot enable permissions. Plans can lower cumulative limits.
The original plan cannot be widened on a retry, including by consistently editing
both saved rows. Host-selected per-batch limits retain their independent bounds.

The response nests a small `worker` execution receipt and the App-bound `history`
envelope. A historical worker replay reports zero new HTTP requests and does not
repeat provider work. The history driver always uses the supplied history revision;
it never adopts the revision returned by creation or status. Stale revisions refuse.

Worker completion, binding/job creation, and successive history batches are separate
durable phases. A later refusal does not undo earlier successful phases. Lost stdout
is recovered by reading worker/history status and explicitly selecting the next
revision; retrying a completed history job is not another execution grant. The
driver reserves wrapper bytes before history mutations. Each history publication
admits its complete nested output inside the owned transaction.

## Separate management and restart

`app-history-job` takes absolute `--profile` and `--selection` host files. Selection
is closed schema 1 with a `completionId` and optional `expectedQueueRevision`.
Create, step, run and resume require the exact current selected queue revision.
Status, pause and cancellation may use the historical completion with current
App/repository grants. They do not revive a superseded selection.

```sh
yoke --root /project --allow-writes --max-bytes 4000000 app-history-job --profile /host/app.json --selection /host/completion.json --allow-app-history --allow-jobs --allow-capture create
yoke --root /project --allow-writes --max-bytes 4000000 app-history-job --profile /host/app.json --selection /host/completion.json --allow-app-history --allow-jobs --allow-capture --trust-git run --revision 0 --max-batches 8
yoke --root /project app-history-job --profile /host/app.json --selection /host/completion.json --allow-app-history --allow-jobs status
```

Creation accepts an optional absolute `--plan` file. Pause, resume and cancel each
take `--revision`. Step/run require trusted Git; other operations reject that flag.
Status uses an existing-only read-only Store, without an Engine, schema creation,
source discovery, Git execution or network. It still replays the authenticated
worker and checked original source. Nested native history status retains its own
`sourceBodiesReplayed:false` for recorded historical capture references. Global
source, analyzer, language-server and snapshot overrides are unavailable.

One deterministic binding/job exists per workspace/App/completion. A domain-separated
HMAC binds the worker job/review, captured head, original request and cumulative
plan. Current and configured prior signing keys support rotation. Binding and job
creation share the existing WAL/FULL-or-EXTRA transaction; output/capacity/SQL failure
rolls both back. Missing sidecars cannot adopt existing generic jobs, and missing
native jobs are not silently recreated. At most eight bindings and 128 KB aggregate
binding JSON are admitted, with 16 KB per row. The native job limits also apply.
Cancellation preserves roots and capacity; it does not delete history.

Retention structurally validates closed typed bindings against actual worker
completion facts, native jobs and checked Git receipts. No OS key lookup occurs
during retention. Structural consistency is not keyed authentication or execution
authority; arbitrary imported strings grant no snapshot/CAS roots.

Results retain `complete:false`, no model calls and no source/latest changes.
History acquisition makes no fresh remote-head observation: only the original
worker execution reports one. Worker per-file Git-blob/patch verification keeps its
finite scope and does not establish atomic whole-tree provenance. Independent
receiver-side source verification/reasoning and configured live App installation
remain separate work. Owned loopback providers are not a production App deployment.
