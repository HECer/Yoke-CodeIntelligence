use std::{fs, path::Path};
use yoke::{
    capture::Deadline,
    repository_discovery::{self, Marker, Options},
};

fn scan(root: &Path, options: &Options) -> anyhow::Result<repository_discovery::Report> {
    repository_discovery::discover(root, options, &Deadline::new(30_000))
}
fn directory(root: &Path, path: &str) {
    fs::create_dir_all(root.join(path)).unwrap();
}

#[test]
fn deterministic_nested_candidates_keep_nearest_marked_parent_and_unicode_names() {
    let root = tempfile::tempdir().unwrap();
    for path in [
        ".git",
        "é π '/outer/.git",
        "é π '/outer/plain/deep/.git",
        "SS/.git",
    ] {
        directory(root.path(), path);
    }
    // Some volumes alias these Unicode case-fold variants. Exclusive creation
    // observes that fact before any second marker or source could be written.
    let distinct = match fs::create_dir(root.path().join("ß")) {
        Ok(()) => {
            directory(root.path(), "ß/.git");
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            assert!(root.path().join("ß/.git").is_dir());
            assert!(
                !fs::read_dir(root.path())
                    .unwrap()
                    .any(|entry| entry.unwrap().file_name() == "ß")
            );
            false
        }
        Err(error) => panic!("Owned Unicode directory creation failed: {error}"),
    };
    // Marker contents, source syntax, executable-looking files and ignore rules
    // are inert. Discovery never initializes the source's own state directory.
    fs::write(
        root.path().join(".git/config"),
        b"[core]\nfsmonitor=execute_me\n",
    )
    .unwrap();
    fs::write(root.path().join(".gitignore"), b"SS/\n").unwrap();
    fs::write(root.path().join("run.ps1"), b"throw 'must not execute'").unwrap();
    fs::write(root.path().join("invalid.rs"), [255, 0, 254]).unwrap();
    let options = Options {
        max_depth: 6,
        ..Default::default()
    };
    let report = scan(root.path(), &options).unwrap();
    assert_eq!(report, scan(root.path(), &options).unwrap());
    let paths = report
        .candidates
        .iter()
        .map(|c| c.path.as_str())
        .collect::<Vec<_>>();
    if distinct {
        assert_eq!(
            paths,
            [".", "SS", "ß", "é π '/outer", "é π '/outer/plain/deep"]
        );
    } else {
        assert_eq!(paths, [".", "SS", "é π '/outer", "é π '/outer/plain/deep"]);
        assert!(report.select("ß").is_err());
    }
    assert_eq!(report.select(".").unwrap().parent, None);
    assert_eq!(report.select("SS").unwrap().parent.as_deref(), Some("."));
    assert_eq!(
        report
            .select("é π '/outer/plain/deep")
            .unwrap()
            .parent
            .as_deref(),
        Some("é π '/outer")
    );
    assert!(
        report
            .candidates
            .iter()
            .all(|c| c.marker == Marker::GitDirectory)
    );
    assert!(!report.complete && !report.registration_granted && !report.source_access_granted);
    assert!(!root.path().join(".yoke").exists());
    assert_eq!(
        fs::read(root.path().join("invalid.rs")).unwrap(),
        [255, 0, 254]
    );
}

#[test]
fn git_files_are_opaque_even_for_missing_external_or_invalid_redirects() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    directory(outside.path(), "private/.git");
    for (path, bytes) in [
        (
            "worktree",
            format!("gitdir: {}", outside.path().display()).into_bytes(),
        ),
        ("missing", b"gitdir: Z:/unavailable/secrets".to_vec()),
        ("invalid", vec![0, 255, 254]),
    ] {
        directory(root.path(), path);
        fs::write(root.path().join(path).join(".git"), &bytes).unwrap();
    }
    let report = scan(root.path(), &Options::default()).unwrap();
    assert_eq!(report.candidates.len(), 3);
    assert!(
        report
            .candidates
            .iter()
            .all(|c| c.marker == Marker::GitFile && c.parent.is_none())
    );
    assert!(
        report
            .candidates
            .iter()
            .all(|c| !c.path.contains("private"))
    );
    assert_eq!(report.directories_visited, 4);
    assert!(!outside.path().join(".yoke").exists());
}

#[test]
fn zero_depth_inspects_root_but_reports_unvisited_child_directories() {
    let root = tempfile::tempdir().unwrap();
    directory(root.path(), ".git");
    directory(root.path(), "child/.git");
    let report = scan(
        root.path(),
        &Options {
            max_depth: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].path, ".");
    assert_eq!(report.directories_visited, 1);
    assert!(
        report
            .omissions
            .iter()
            .any(|o| o.path == "child" && o.reason == "depth-limit")
    );
    assert!(report.select("child").is_err());
}

#[test]
fn exact_caps_accept_one_root_marker_and_uppercase_metadata_stays_protected() {
    let root = tempfile::tempdir().unwrap();
    directory(root.path(), ".GIT/private/.git");
    let options = Options {
        max_depth: 0,
        max_entries: 1,
        max_directories: 1,
        max_repositories: 1,
        ..Default::default()
    };
    let report = scan(root.path(), &options).unwrap();
    assert_eq!(report.entries_visited, 1);
    assert_eq!(report.directories_visited, 1);
    // Marker lookup follows the volume's alias behavior; enumeration uses the
    // scanner's platform-specific spelling rule independently.
    if root.path().join(".git").is_dir() {
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].path, ".");
    } else {
        assert!(report.candidates.is_empty());
    }
    if cfg!(windows) {
        assert!(report.omissions.is_empty());
    } else {
        assert_eq!(report.omissions.len(), 1);
        assert_eq!(report.omissions[0].path, ".GIT");
        assert_eq!(report.omissions[0].reason, "protected-path");
    }
    assert!(report.select(".GIT").is_err());
    assert!(report.select(".GIT/private").is_err());
    assert!(report.select(".git/private").is_err());
    assert_eq!(
        scan(&root.path().join(".GIT/private"), &options)
            .unwrap_err()
            .to_string(),
        "repository_discovery_protected_root"
    );
    assert!(!report.complete);
    assert!(!report.registration_granted);
    assert!(!report.source_access_granted);
}

#[test]
fn protected_trees_are_never_candidates_or_descended() {
    let root = tempfile::tempdir().unwrap();
    for path in [
        "node_modules/pkg/.git",
        "target/debug/.git",
        ".yoke/state/.git",
        ".env.private/.git",
        ".ssh/private/.git",
        ".venv/pkg/.git",
    ] {
        directory(root.path(), path);
    }
    let report = scan(root.path(), &Options::default()).unwrap();
    assert!(report.candidates.is_empty());
    assert_eq!(report.directories_visited, 1);
    assert_eq!(report.omissions.len(), 6);
    assert!(
        report
            .omissions
            .iter()
            .all(|o| o.reason == "protected-path")
    );
    assert!(scan(&root.path().join("node_modules"), &Options::default()).is_err());
}

#[test]
fn budgets_refuse_instead_of_returning_truncated_empty_success() {
    let root = tempfile::tempdir().unwrap();
    directory(root.path(), "one/.git");
    directory(root.path(), "two/.git");
    for options in [
        Options {
            max_entries: 1,
            ..Default::default()
        },
        Options {
            max_directories: 1,
            ..Default::default()
        },
        Options {
            max_repositories: 1,
            ..Default::default()
        },
        Options {
            max_repositories: 0,
            ..Default::default()
        },
        Options {
            max_report_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(
            scan(root.path(), &options)
                .unwrap_err()
                .to_string()
                .contains("capacity_exceeded")
        );
    }
    let empty = tempfile::tempdir().unwrap();
    assert!(
        scan(
            empty.path(),
            &Options {
                max_repositories: 0,
                ..Default::default()
            }
        )
        .unwrap()
        .candidates
        .is_empty()
    );
    assert!(!root.path().join(".yoke").exists());
}

#[test]
fn malformed_limits_expired_deadline_and_arbitrary_selectors_refuse() {
    let root = tempfile::tempdir().unwrap();
    directory(root.path(), ".git");
    for options in [
        Options {
            max_depth: 33,
            ..Default::default()
        },
        Options {
            max_entries: 0,
            ..Default::default()
        },
        Options {
            max_entries: 100_001,
            ..Default::default()
        },
        Options {
            max_directories: 0,
            ..Default::default()
        },
        Options {
            max_directories: 10_001,
            ..Default::default()
        },
        Options {
            max_repositories: 1_001,
            ..Default::default()
        },
        Options {
            max_report_bytes: 0,
            ..Default::default()
        },
        Options {
            max_report_bytes: 4_000_001,
            ..Default::default()
        },
    ] {
        assert!(scan(root.path(), &options).is_err());
    }
    assert!(
        repository_discovery::discover(root.path(), &Options::default(), &Deadline::new(0))
            .is_err()
    );
    assert!(scan(Path::new("."), &Options::default()).is_err());
    assert!(scan(&root.path().join("../"), &Options::default()).is_err());
    let report = scan(root.path(), &Options::default()).unwrap();
    for selector in [
        "",
        "../",
        "./",
        "/outside",
        "one/../two",
        "C:/outside",
        "one\\two",
        "missing",
        ".git",
        "a\0b",
        "a\nb",
        "a.",
    ] {
        assert!(report.select(selector).is_err(), "{selector:?}");
    }
    let encoded = serde_json::to_value(Options::default()).unwrap();
    let mut unknown = encoded;
    unknown["executeGit"] = true.into();
    assert!(serde_json::from_value::<Options>(unknown).is_err());
}

#[test]
fn large_omission_lists_obey_incremental_output_budget() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..30 {
        directory(
            root.path(),
            &format!("long-directory-{i:03}-{}", "x".repeat(40)),
        );
    }
    let options = Options {
        max_depth: 0,
        max_report_bytes: 2_000,
        ..Default::default()
    };
    assert!(
        scan(root.path(), &options)
            .unwrap_err()
            .to_string()
            .contains("capacity_exceeded")
    );
    assert!(!root.path().join(".yoke").exists());
}

#[cfg(unix)]
#[test]
fn source_and_marker_symlinks_remain_omissions_and_root_links_refuse() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    directory(outside.path(), ".git");
    std::os::unix::fs::symlink(outside.path(), root.path().join("outside")).unwrap();
    directory(root.path(), "marker");
    std::os::unix::fs::symlink(outside.path(), root.path().join("marker/.git")).unwrap();
    let report = scan(root.path(), &Options::default()).unwrap();
    assert!(report.candidates.is_empty());
    assert!(
        report
            .omissions
            .iter()
            .any(|o| o.path == "outside" && o.reason == "link-or-reparse-point")
    );
    assert!(
        report
            .omissions
            .iter()
            .any(|o| o.path == "marker/.git" && o.reason == "marker-link-or-reparse-point")
    );
    assert!(scan(&root.path().join("outside"), &Options::default()).is_err());
}
