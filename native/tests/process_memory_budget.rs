use yoke::process::{memory_limit_kind, validate_memory_report};

#[test]
fn native_policy_requires_the_matching_kind_and_platform_specific_accounting() {
    let bytes = 256 * 1024 * 1024;
    assert!(validate_memory_report(0, memory_limit_kind(), None, 0).is_err());
    assert!(validate_memory_report(bytes, "invented_policy", None, bytes).is_err());
    let legacy = validate_memory_report(bytes, memory_limit_kind(), None, bytes);
    assert_eq!(legacy.is_err(), cfg!(target_os = "macos"));
    #[cfg(not(target_os = "macos"))]
    assert!(validate_memory_report(bytes + 1, memory_limit_kind(), None, bytes).is_err());
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{io::Write, process::Command};
    use yoke::{
        capture::Deadline,
        process::{self, CaptureLimits},
    };
    const ROLE: &str = "YOKE_OWNED_MACOS_MEMORY_PROBE";
    const BUDGET: usize = 256 * 1024 * 1024;

    #[test]
    fn budget_probe_child() {
        if std::env::var(ROLE).as_deref() != Ok("owned") {
            return;
        }
        let memory = process::limit_current_memory(BUDGET).unwrap();
        let first = memory.ceiling_bytes();
        assert_eq!(process::current_memory_limit().unwrap(), Some(first));
        process::validate_memory_report(
            first,
            process::memory_limit_kind(),
            memory.macos_budget(),
            BUDGET,
        )
        .unwrap();
        // The kernel must deny a fresh mapping larger than all allowed growth,
        // without touching any pages or relying on allocator arena behavior.
        let mapping = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                2 * BUDGET,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if mapping != libc::MAP_FAILED {
            unsafe {
                libc::munmap(mapping, 2 * BUDGET);
            }
            panic!("macOS kernel admitted a mapping beyond the growth budget");
        }
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ENOMEM)
        );
        let small = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        assert_ne!(small, libc::MAP_FAILED);
        unsafe {
            small.cast::<u8>().write_volatile(7);
            assert_eq!(libc::munmap(small, 4096), 0);
        }
        let repeated = process::limit_current_memory(BUDGET).unwrap();
        assert!(
            repeated.ceiling_bytes() <= first,
            "Repeated limiting refreshed the allowance"
        );
        // A pre-existing tighter ceiling remains tighter; no implicit fallback
        // may raise it to restore a full new allowance.
        let report = serde_json::to_value(repeated.macos_budget().unwrap()).unwrap();
        let baseline = report["baselineVirtualBytes"].as_u64().unwrap() as usize;
        let tight = baseline + (repeated.ceiling_bytes() - baseline) / 2;
        let limit = libc::rlimit {
            rlim_cur: tight as _,
            rlim_max: tight as _,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) }, 0);
        let inherited = process::limit_current_memory(BUDGET).unwrap();
        assert!(inherited.ceiling_bytes() <= tight);
        assert_eq!(
            process::current_memory_limit().unwrap(),
            Some(inherited.ceiling_bytes())
        );
        let evidence = serde_json::to_vec(memory.macos_budget().unwrap()).unwrap();
        std::io::stdout().write_all(b"MACOS_BUDGET=").unwrap();
        std::io::stdout().write_all(&evidence).unwrap();
        std::io::stdout().write_all(b"\n").unwrap();
    }

    fn probe(parent_limit: bool) {
        let root = tempfile::tempdir().unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .current_dir(root.path())
            .args(["--exact", "macos::budget_probe_child", "--nocapture"])
            .env(ROLE, "owned");
        let cap = CaptureLimits {
            stdin_bytes: 0,
            stdout_bytes: 16_384,
            stderr_bytes: 16_384,
        };
        let deadline = Deadline::new(20_000);
        let result = if parent_limit {
            process::run_capture_with_memory_limit(command, b"", &deadline, cap, BUDGET)
        } else {
            process::run_capture(command, b"", &deadline, cap)
        }
        .unwrap();
        assert_eq!(
            result.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            result.output_complete
                && !result.timed_out
                && !result.io_error
                && !result.output_exceeded
        );
        let text = std::str::from_utf8(&result.stdout).unwrap();
        let report = text
            .lines()
            .find_map(|line| line.strip_prefix("MACOS_BUDGET="))
            .unwrap_or_else(|| panic!("Missing native budget observation: {text}"));
        let report: process::MacosMemoryBudget = serde_json::from_str(report).unwrap();
        report.validate(BUDGET).unwrap();
    }
    #[test]
    fn direct_worker_installs_and_enforces_growth_without_refreshing_or_raising_inherited_caps() {
        probe(false);
    }
    #[test]
    fn parent_pre_exec_and_worker_self_limits_both_enforce_growth() {
        probe(true);
    }
}
