//! Git's default walk is a date-priority traversal, not a global date sort.
use super::*;

fn dated_commit(
    host: &HostGit,
    root: &Path,
    tree: &str,
    parents: &[&str],
    date: i64,
    author_date: i64,
    label: &str,
) -> String {
    let parent_lines = parents
        .iter()
        .map(|id| format!("parent {id}\n"))
        .collect::<String>();
    let raw = format!(
        "tree {tree}\n{parent_lines}author Fixture <fixture@example.invalid> {author_date} +0000\ncommitter Fixture <fixture@example.invalid> {date} +0000\n\n{label}\n"
    );
    commit_bytes(host, root, raw.as_bytes())
}

fn assert_git_default_order(case: &str) {
    let mut expected = Vec::new();
    let f = fixture(|host, root, tree, _| {
        // These histories share the captured tree, independently of live HEAD.
        let (left, right, head_date) = if case == "skew" {
            let ancestor = dated_commit(host, root, tree, &[], 1000, 1, "root");
            let left = dated_commit(host, root, tree, &[&ancestor], 200, 900, "left");
            let right_old = dated_commit(host, root, tree, &[&ancestor], 100, 700, "right-old");
            let right = dated_commit(host, root, tree, &[&right_old], 300, 600, "right");
            expected = vec![right_old, ancestor, left.clone(), right.clone()];
            (left, right, 400)
        } else if case == "negative" {
            let ancestor = dated_commit(host, root, tree, &[], 10, 1, "root");
            let left = dated_commit(host, root, tree, &[&ancestor], -1, 2, "negative");
            let right = dated_commit(host, root, tree, &[&ancestor], 0, 3, "zero");
            expected = vec![right.clone(), ancestor, left.clone()];
            (left, right, 1)
        } else {
            let date = |value| if case == "equal" { 2 } else { value };
            let ancestor = dated_commit(host, root, tree, &[], date(10), 1000, "root");
            let left_old = dated_commit(host, root, tree, &[&ancestor], date(20), 900, "left-old");
            let left = dated_commit(host, root, tree, &[&left_old], date(40), 800, "left");
            let right_old =
                dated_commit(host, root, tree, &[&ancestor], date(30), 700, "right-old");
            let right = dated_commit(host, root, tree, &[&right_old], date(50), 600, "right");
            expected = if case == "equal" {
                vec![ancestor, right_old, left_old, right.clone(), left.clone()]
            } else {
                vec![ancestor, left_old, right_old, left.clone(), right.clone()]
            };
            (left, right, date(60))
        };
        dated_commit(host, root, tree, &[&left, &right], head_date, 1, "merge")
    });
    let before = files(&f.root);
    for maximum in [1000, 1, 2] {
        let maximum_arg = maximum.to_string();
        // This independent oracle walks only the six owned fixture objects;
        // production must not replace its acquisition caps with a rev-list cap.
        let oracle = git(
            &f.host,
            &f.root,
            &[
                "log",
                "--no-merges",
                "--reverse",
                "-n",
                &maximum_arg,
                "--format=%H",
                &f.head,
            ],
        );
        let selected = &expected[expected.len().saturating_sub(maximum)..];
        assert_eq!(
            oracle.lines().collect::<Vec<_>>(),
            selected.iter().map(String::as_str).collect::<Vec<_>>(),
            "Owned Git fixture no longer represents {case}"
        );
        let (actual, _) = value(history(&f).arg("--max-commits").arg(&maximum_arg));
        assert_eq!(
            actual["commits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["sha"].as_str().unwrap())
                .collect::<Vec<_>>(),
            selected.iter().map(String::as_str).collect::<Vec<_>>(),
            "History must follow Git's default date-priority walk for {case}, max={maximum}"
        );
        assert_eq!(actual["history"]["headCommitId"], f.head);
        assert_eq!(actual["providerVerified"], false);
        assert_eq!(actual["complete"], false);
    }
    assert_eq!(files(&f.root), before);
}

#[test]
fn commit_date_priority_matches_git_and_ignores_reversed_author_dates() {
    assert_git_default_order("monotone");
}

#[test]
fn clock_skew_preserves_git_discovery_order_instead_of_globally_sorting_dates() {
    assert_git_default_order("skew");
}

#[test]
fn equal_commit_dates_preserve_git_parent_insertion_order() {
    assert_git_default_order("equal");
}

#[test]
fn negative_stored_commit_time_uses_gits_unsigned_timestamp_order() {
    assert_git_default_order("negative");
}
