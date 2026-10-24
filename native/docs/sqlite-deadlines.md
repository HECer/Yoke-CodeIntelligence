# Checked Store lock waits and publication deadlines

`Store::set_deadline` and checked Store operations install a connection-local
SQLite busy handler alongside the existing SQL progress handler. Lock waits
consult the original `Deadline`, including its shared host cancellation flags,
before and after each requested sleep of at most two milliseconds. A locking
event also retains the existing ten-second ceiling. Time already spent on
preparation consumes the same absolute request budget.

SQLite's built-in timeout counts requested sleep durations. Actual scheduling
can delay resumption beyond those durations; the new handler checks actual
monotonic time when it resumes. It cannot make a descheduled thread run. SQLite
can still return `SQLITE_BUSY` without invoking a handler to avoid a deadlock;
this change does not retry a transaction or conceal that refusal. The callback
returns a normal busy refusal when its time/cancellation policy is exhausted.

The callback argument is SQLite's own connection handle. The locked bundled
SQLite owns the Rust policy allocation through
[`sqlite3_set_clientdata`](https://sqlite.org/c3ref/get_clientdata.html), including
registration failure and connection closure. A callback performs only a
read-only lookup of that private registration, checks time/atomic flags and
sleeps. It issues no SQL and modifies no connection state. If closing or explicit
removal has removed the registration, it refuses immediately. No Store address,
thread-local deadline or global callback slot is retained. Moving a public
`Store::db` out of its Store and onto another thread preserves its own policy.
The implementation follows the
[busy-handler contract](https://sqlite.org/c3ref/busy_handler.html).

`clear_deadline` removes the SQL progress policy and clears the owned busy
policy's request deadline/cancellation flags. Subsequent ordinary writes can
recover under the finite default wait. A custom busy handler makes SQLite's
`PRAGMA busy_timeout` report zero; this is not a statement that the custom wait
has no bound. Existing digest policy guards still restore their caller's numeric
busy timeout and SQL length/progress policy.

Generic record creation, conditional replacement and conditional deletion
serialize their host values before a second deadline/cancellation check. A
serializer that consumes the remaining time or triggers host cancellation cannot
then publish a record mutation. Serialization itself remains cooperative host
code; it is not preempted. Ledger transaction admission keeps the checked Store
policy instead of replacing it with a separate relative SQLite timeout. No new
reservation, cost, source or recovery authority is introduced.

The writable Store constructor now uses the same absolute opening deadline for
client admission, client lease acquisition and initial schema/identity work.
Its full `Duration` inherits any enclosing host ceiling and cancellation flags;
zero or unrepresentable durations and already cancelled requests refuse before
creating `.yoke`. The SQL busy and progress handlers are installed before the
first setup statement. Successful opening clears this setup policy so future
unrelated Store operations select their own request deadline. Initialization
remains cooperative and can retain partial setup if a later step fails.

Existing-only read-only Store setup keeps its explicit first-SQL retry loop.
Other independent raw SQLite connections and deliberately selected raw
connection policies remain separate. Full platform acceptance requires the
corresponding hosted runs.

Real SQLite regressions cover cancellation during a longer lock wait, elapsed
serialization budgets, uncontended serialization-triggered cancellation for all
three record mutations, cancelled ledger admission with unchanged accounting,
brief contention followed by exact Unicode publication, recovery and independent
policy ownership after moving a connection. A module test removes and
reinstalls callback registration to exercise the fail-closed lifetime boundary.
Separate opening tests cover zero/extreme durations, cancellation before state
creation, actual rollback-journal reader exclusion, both physical lease files,
the inherited host ceiling, recovery, preserved workspace/Unicode data and setup
policy disposal after a successful opening.
