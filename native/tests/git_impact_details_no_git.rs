use yoke as test_yoke;
#[path = "support/git_impact_fixture.rs"]
mod fixture;
use fixture::{Fixture, state};
use std::{ffi::OsString, path::Path};
use yoke::{
    capture::Deadline,
    git_workflows::impact_details::{self, DetailOptions},
};

// This separate one-test integration process is the only target that changes
// PATH. Its sole thread restores the value on unwind; no parallel fixture here.
struct PathGuard(Option<OsString>);
impl PathGuard {
    fn unavailable() -> Self {
        let previous = std::env::var_os("PATH");
        // SAFETY: This integration executable contains one test; its real Git
        // fixture and any inherited workers have completed before this change.
        unsafe {
            std::env::set_var("PATH", "");
        }
        Self(previous)
    }
}
impl Drop for PathGuard {
    fn drop(&mut self) {
        // SAFETY: The same sole test thread restores its own environment change.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("PATH", value),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}

fn assert_no_markers(root: &Path) {
    for name in ["r5-git-called", "r5-hook-called", "r5-provider-called"] {
        assert!(!root.join(name).exists());
    }
}

#[test]
fn historical_details_succeed_without_host_path_git_and_ignore_repo_execution_traps() {
    let f = Fixture::moved();
    let baseline = impact_details::derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .expect("genuine feature before unavailable-tool gate");
    let git_dir = f.dir.path().join(".git");
    assert!(git_dir.is_dir(), "genuine fixture Git directory");
    std::fs::create_dir_all(git_dir.join("hooks")).unwrap();
    std::fs::write(
        f.dir.path().join("git.cmd"),
        "@echo called>r5-git-called\r\n@exit /b 93\r\n",
    )
    .unwrap();
    std::fs::write(
        f.dir.path().join(".git/hooks/post-checkout"),
        "#!/bin/sh\necho called >r5-hook-called\nexit 93\n",
    )
    .unwrap();
    let before = state(&f.engine);
    let _path = PathGuard::unavailable();
    let output = impact_details::derive(
        &f.engine.store,
        f.id(),
        &DetailOptions::default(),
        &Deadline::new(30_000),
    )
    .unwrap();
    assert_eq!(output.canonical_bytes(), baseline.canonical_bytes());
    assert_eq!(state(&f.engine), before);
    assert_no_markers(f.dir.path());
}
