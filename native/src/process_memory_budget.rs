//! macOS reserves large VM regions before worker input. This policy bounds
//! additional virtual address space, not RSS, committed bytes or allocator use
//! inside already mapped regions. Reports must keep that distinction explicit.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MACOS_MEMORY_KIND: &str = "macos_rlimit_as_growth_v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MacosMemoryBudget {
    schema: u8,
    baseline_virtual_bytes: u64,
    growth_budget_bytes: u64,
    ceiling_bytes: u64,
}
impl MacosMemoryBudget {
    pub fn ceiling_bytes(&self) -> u64 {
        self.ceiling_bytes
    }
    pub fn validate(&self, requested: usize) -> Result<()> {
        ensure!(
            self.schema == 1
                && (32 * 1024 * 1024..=2 * 1024 * 1024 * 1024).contains(&requested)
                && self.growth_budget_bytes == requested as u64
                && self.baseline_virtual_bytes > 0
                && self.ceiling_bytes < i64::MAX as u64
                && self.ceiling_bytes > self.baseline_virtual_bytes
                && self
                    .baseline_virtual_bytes
                    .checked_add(self.growth_budget_bytes)
                    .is_some_and(|maximum| self.ceiling_bytes <= maximum),
            "Invalid macOS virtual-address growth budget"
        );
        Ok(())
    }
}

#[cfg(any(target_os = "macos", test))]
fn plan(baseline: u64, requested: usize, inherited: u64) -> Result<MacosMemoryBudget> {
    let ceiling = baseline
        .checked_add(requested as u64)
        .ok_or_else(|| anyhow::anyhow!("macOS virtual-address budget overflow"))?
        .min(inherited);
    let budget = MacosMemoryBudget {
        schema: 1,
        baseline_virtual_bytes: baseline,
        growth_budget_bytes: requested as u64,
        ceiling_bytes: ceiling,
    };
    budget.validate(requested)?;
    Ok(budget)
}

#[cfg(target_os = "macos")]
pub(super) fn observe_and_plan(requested: usize) -> Result<MacosMemoryBudget> {
    use anyhow::Context;
    // libc supplies the SDK ABI (including Mach's four-byte structure packing).
    let mut info = std::mem::MaybeUninit::<libc::mach_task_basic_info>::zeroed();
    let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
    // The pinned libc keeps this Mach self-port wrapper deprecated in favour
    // of mach2; use its existing SDK ABI without adding a second FFI dependency.
    #[allow(deprecated)]
    let status = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    ensure!(
        status == libc::KERN_SUCCESS && count == libc::MACH_TASK_BASIC_INFO_COUNT,
        "macOS virtual-address observation unavailable; machStatus={status}"
    );
    let baseline = unsafe { info.assume_init() }.virtual_size;
    let mut inherited = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_AS, &mut inherited) } != 0 {
        return Err(std::io::Error::last_os_error())
            .context("macOS inherited RLIMIT_AS unavailable");
    }
    plan(
        baseline,
        requested,
        inherited.rlim_cur.min(inherited.rlim_max),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const BUDGET: usize = 256 * 1024 * 1024;
    #[test]
    fn large_baselines_are_separate_from_growth_and_never_refresh_inherited_limits() {
        let baseline = 415 * 1024_u64.pow(3);
        let first = plan(baseline, BUDGET, u64::MAX).unwrap();
        assert_eq!(first.ceiling_bytes(), baseline + BUDGET as u64);
        let second = plan(baseline + 4096, BUDGET, first.ceiling_bytes()).unwrap();
        assert_eq!(second.ceiling_bytes(), first.ceiling_bytes());
        assert!(plan(first.ceiling_bytes(), BUDGET, first.ceiling_bytes()).is_err());
        let tighter = plan(baseline, BUDGET, baseline + 8192).unwrap();
        assert_eq!(tighter.ceiling_bytes(), baseline + 8192);
    }
    #[test]
    fn overflow_invalid_bounds_and_forged_or_unknown_report_fields_refuse() {
        for (base, size, inherited) in [
            (u64::MAX - 1, BUDGET, u64::MAX),
            (0, BUDGET, u64::MAX),
            (1024, 0, u64::MAX),
            (1024, usize::MAX, u64::MAX),
            (1024, BUDGET, 1023),
        ] {
            assert!(plan(base, size, inherited).is_err());
        }
        let original = plan(4096, BUDGET, u64::MAX).unwrap();
        let mut value = serde_json::to_value(&original).unwrap();
        for (key, bad) in [
            ("schema", 2),
            ("growthBudgetBytes", 1),
            ("baselineVirtualBytes", 0),
            ("ceilingBytes", u64::MAX),
        ] {
            let mut forged = value.clone();
            forged[key] = serde_json::json!(bad);
            assert!(
                serde_json::from_value::<MacosMemoryBudget>(forged)
                    .unwrap()
                    .validate(BUDGET)
                    .is_err()
            );
        }
        value["rssLimitBytes"] = serde_json::json!(BUDGET);
        assert!(serde_json::from_value::<MacosMemoryBudget>(value).is_err());
        let infinite = serde_json::json!({"schema":1,
            "baselineVirtualBytes": i64::MAX as u64 - BUDGET as u64,
            "growthBudgetBytes": BUDGET, "ceilingBytes": i64::MAX as u64});
        assert!(
            serde_json::from_value::<MacosMemoryBudget>(infinite)
                .unwrap()
                .validate(BUDGET)
                .is_err()
        );
    }
}
