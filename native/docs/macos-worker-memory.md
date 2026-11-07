# macOS worker virtual-address growth budget

The native template and PDF workers use `macos_rlimit_as_growth_v1` on macOS.
The requested 256 MiB is additional virtual address space, not a 256 MiB total
address-space, resident-memory, committed-memory or heap-allocation ceiling.
In particular, allocations can reuse regions reserved before the baseline.
This is intentionally different from Linux's absolute `RLIMIT_AS` and Windows
Job committed-memory policy; those policies and their existing wire fields stay
unchanged.

For memory-limited capture (used by the template parent), before fork the parent
observes its own virtual size with
`MACH_TASK_BASIC_INFO` and reads its inherited soft/hard `RLIMIT_AS`. The planned
ceiling is the smaller of baseline plus budget and both inherited limits.
Only `setrlimit` runs in the post-fork hook. It installs the absolute soft and
hard ceiling before exec. Observation, overflow, insufficient headroom or limit
installation failures refuse execution; there is no unlimited fallback.

Each dedicated worker calculates its own budget against its startup mappings
and inherited limits before reading input, installs both limits and verifies
the actual OS ceiling. PDF workers keep their existing self-limiting lifecycle;
only memory-limited capture adds a pre-exec limit. A worker can tighten a parent's
allowance but cannot raise
either inherited limit. Repeated calls do not replenish the allowance. Startup
or concurrent mapping growth that cannot fit the installed ceiling fails closed.
The policy does not subtract an estimated dyld size or trust an environment or
request-supplied baseline. It does not provide an OS sandbox.

Template replies retain `memoryCeilingBytes` as the **actual absolute ceiling**.
PDF resource metadata likewise reports the absolute ceiling in `memoryBytes`.
Only macOS adds this closed `memoryBudget` record:

```json
{
  "schema": 1,
  "baselineVirtualBytes": 445745856512,
  "growthBudgetBytes": 268435456,
  "ceilingBytes": 446014291968
}
```

These are illustrative numbers, not a constant baseline. The ceiling must exceed
the observed baseline by no more than the requested budget, and can be lower
when an inherited limit is tighter. Parents validate the kind, record schema,
budget arithmetic and matching ceiling before accepting worker output. Reported
worker evidence is not independent OS attestation. Old immutable stored records
remain literal; they are never relabelled with this new policy.

Pure regression cases cover overflow, forged records, large baselines and
nonrefreshing inherited ceilings. Native macOS cases exercise direct and
parent-limited child processes, exact OS observation, small mapping success,
oversized mapping denial and inherited-limit preservation. The public PDF and
template cases assert the platform-specific metadata. Native macOS qualification
requires those cases to actually run on the current source; it is pending until
hosted CI verifies this candidate.
