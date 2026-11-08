# SQLite request cancellation

Store deadlines retain the complete host `Deadline`, including inherited cancellation flags. The SQLite progress callback checks that same bound every 1,000 virtual-machine operations. Store operation preflight and snapshot publication checkpoints also inspect cancellation. Clearing or replacing the deadline resets the connection for an independently authorized operation.

SQLite lock contention remains bounded by the existing busy timeout, up to the remaining request budget and at most ten seconds. SQLite does not invoke the progress callback while its built-in busy handler waits for a lock. Cancellation during that wait therefore does not promise immediate interruption; the finite busy timeout still applies. Contended writes retain their original atomicity and recovery behavior.

The pinned rusqlite safe busy-handler API accepts a function pointer without a captured per-connection context. This change does not substitute global thread-local cancellation state, shorten legitimate lock-wait budgets, or add an unreviewed callback pointer lifetime. A future cancellable busy handler requires a separately reviewed per-connection ownership design.
