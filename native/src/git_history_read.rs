//! Size-admitted commit-object traversal; independent of Git revision walkers.
use crate::capture::Deadline;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
};

#[derive(Clone)]
pub struct Options {
    pub max_commits: usize,
    pub max_visits: usize,
    pub max_frontier: usize,
    pub max_parent_ids: usize,
    pub max_commit_bytes: usize,
    pub max_history_bytes: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_commits: 1000,
            max_visits: 4000,
            max_frontier: 4000,
            max_parent_ids: 16_000,
            max_commit_bytes: 1_000_000,
            max_history_bytes: 64_000_000,
        }
    }
}
#[derive(Debug)]
pub(crate) struct Capacity;
impl std::fmt::Display for Capacity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("github_app_digest_capacity_exceeded")
    }
}
impl std::error::Error for Capacity {}
const UNAVAILABLE: &str = "github_app_digest_history_unavailable";
impl Options {
    pub fn validate(&self) -> Result<()> {
        let hard = Self::default();
        ensure!(
            self.max_commits <= hard.max_commits
                && self.max_visits <= hard.max_visits
                && self.max_frontier <= hard.max_frontier
                && self.max_parent_ids <= hard.max_parent_ids
                && self.max_commit_bytes <= hard.max_commit_bytes
                && self.max_history_bytes <= hard.max_history_bytes,
            Capacity
        );
        Ok(())
    }
}
trait Objects {
    fn size(&mut self, id: &str, d: &Deadline) -> Result<usize>;
    fn contents(&mut self, id: &str, maximum: usize, d: &Deadline) -> Result<Vec<u8>>;
}
#[derive(Debug)]
pub(crate) struct History {
    pub(crate) commits: Vec<Value>,
    pub(crate) summary: Value,
}
struct Commit {
    parents: Vec<String>,
    subject: String,
    body: String,
}
fn parse(raw: &[u8], width: usize, remaining_parents: &mut usize, d: &Deadline) -> Result<Commit> {
    d.check()?;
    let text = std::str::from_utf8(raw).context(UNAVAILABLE)?;
    let (header, message) = text.split_once("\n\n").context(UNAVAILABLE)?;
    ensure!(
        !header
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t')),
        UNAVAILABLE
    );
    let mut parents = Vec::new();
    let mut unique = BTreeSet::new();
    let (mut tree, mut author, mut committer, mut encoding, mut continuation) =
        (false, false, false, false, false);
    let mut parent_phase = true;
    for (index, line) in header.split('\n').enumerate() {
        d.check()?;
        if line.starts_with(' ') {
            ensure!(continuation, UNAVAILABLE);
            continue;
        }
        let (key, value) = line.split_once(' ').context(UNAVAILABLE)?;
        ensure!(
            !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            UNAVAILABLE
        );
        continuation = matches!(key, "gpgsig" | "gpgsig-sha256" | "mergetag");
        if !matches!(key, "tree" | "parent") {
            parent_phase = false;
        }
        match key {
            "tree" => {
                ensure!(
                    index == 0 && !tree && super::oid_valid(value) && value.len() == width,
                    UNAVAILABLE
                );
                tree = true;
            }
            "parent" => {
                ensure!(
                    parent_phase
                        && tree
                        && super::oid_valid(value)
                        && value.len() == width
                        && unique.insert(value),
                    UNAVAILABLE
                );
                ensure!(*remaining_parents > 0, Capacity);
                *remaining_parents -= 1;
                parents.push(value.to_owned());
            }
            "author" => {
                ensure!(!author && !value.is_empty(), UNAVAILABLE);
                author = true;
            }
            "committer" => {
                ensure!(!committer && !value.is_empty(), UNAVAILABLE);
                committer = true;
            }
            "encoding" => {
                ensure!(
                    !encoding && value.eq_ignore_ascii_case("UTF-8"),
                    UNAVAILABLE
                );
                encoding = true;
            }
            _ => {}
        }
    }
    ensure!(tree && author && committer, UNAVAILABLE);
    let message = message.trim();
    let mut subject = String::new();
    let mut offset = 0;
    let mut body_start = message.len();
    for line in message.split_inclusive('\n') {
        d.check()?;
        if line.trim().is_empty() {
            body_start = offset + line.len();
            break;
        }
        if !subject.is_empty() {
            subject.push(' ');
        }
        subject.push_str(line.trim());
        offset += line.len();
    }
    let body = message[body_start..].trim().to_owned();
    d.check()?;
    Ok(Commit {
        parents,
        subject,
        body,
    })
}
fn walk(
    objects: &mut impl Objects,
    head: &str,
    options: &Options,
    d: &Deadline,
) -> Result<History> {
    d.check()?;
    options.validate()?;
    ensure!(super::oid_valid(head), UNAVAILABLE);
    ensure!(options.max_frontier > 0, Capacity);
    let mut frontier = VecDeque::from([head.to_owned()]);
    let mut seen = BTreeSet::from([head.to_owned()]);
    let (mut visited, mut bytes, mut merges, mut empty) = (0usize, 0usize, 0usize, 0usize);
    let mut remaining_parents = options.max_parent_ids;
    let mut commits = Vec::new();
    let mut stopped = None;
    while !frontier.is_empty() {
        d.check()?;
        if commits.len() >= options.max_commits {
            stopped = Some("commit_limit");
            break;
        }
        if visited >= options.max_visits {
            stopped = Some("visit_limit");
            break;
        }
        let id = frontier.pop_front().context(UNAVAILABLE)?;
        let size = objects.size(&id, d)?;
        d.check()?;
        ensure!(
            size <= options.max_commit_bytes
                && size <= options.max_history_bytes.saturating_sub(bytes),
            Capacity
        );
        // Reserve every visited object, including merges, before requesting its
        // contents. Rejected information never grants a body read.
        bytes += size;
        let raw = objects.contents(&id, size, d)?;
        d.check()?;
        ensure!(
            raw.len() == size && super::object_id("commit", &raw, head.len()) == id,
            UNAVAILABLE
        );
        let parsed = parse(&raw, head.len(), &mut remaining_parents, d)?;
        visited += 1;
        for parent in &parsed.parents {
            d.check()?;
            if !seen.contains(parent) {
                ensure!(frontier.len() < options.max_frontier, Capacity);
                seen.insert(parent.clone());
                frontier.push_back(parent.clone());
            }
        }
        if parsed.parents.len() > 1 {
            merges += 1;
        } else if parsed.subject.is_empty() {
            empty += 1;
        } else {
            commits.push(json!({"sha":id,"subject":parsed.subject,"body":parsed.body,
                "parentIds":parsed.parents,"objectVerified":true,
                "files":[],"filesStatus":"unavailable","symbols":[],"symbolsStatus":"unavailable"}));
        }
    }
    commits.reverse();
    d.check()?;
    Ok(History {
        summary: json!({"selectionOrder":"breadth_first_parent_order","outputOrder":"reverse_selection_order",
        "headCommitId":head,"visitedCommits":visited,"selectedCommits":commits.len(),"rawCommitBytes":bytes,
        "mergeCommitsSkipped":merges,"emptyMessagesSkipped":empty,
        "parentIdsObserved":options.max_parent_ids-remaining_parents,"frontierCommits":frontier.len(),
        "exhausted":frontier.is_empty(),"truncationReason":stopped,
        "filesStatus":"unavailable","symbolsStatus":"unavailable","authorshipVerified":false,
        "providerVerified":false,"complete":false}),
        commits,
    })
}
struct GitObjects<'a> {
    git: &'a super::HostGit,
    root: &'a Path,
}
impl Objects for GitObjects<'_> {
    fn size(&mut self, id: &str, d: &Deadline) -> Result<usize> {
        let input = format!("{id}\n");
        let raw = self.git.run(
            self.root,
            &["cat-file", "--batch-check"],
            input.as_bytes(),
            200,
            d,
        )?;
        let text = std::str::from_utf8(&raw).context(UNAVAILABLE)?;
        let prefix = format!("{id} commit ");
        let value = text
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix('\n'))
            .context(UNAVAILABLE)?;
        ensure!(
            !value.is_empty()
                && value.bytes().all(|b| b.is_ascii_digit())
                && (value == "0" || !value.starts_with('0')),
            UNAVAILABLE
        );
        value
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!(Capacity))
    }
    fn contents(&mut self, id: &str, maximum: usize, d: &Deadline) -> Result<Vec<u8>> {
        let input = format!("{id}\n");
        let raw = self.git.run(
            self.root,
            &["cat-file", "--batch"],
            input.as_bytes(),
            maximum + 100,
            d,
        )?;
        let prefix = format!("{id} commit {maximum}\n");
        ensure!(
            raw.starts_with(prefix.as_bytes())
                && raw.len() == prefix.len() + maximum + 1
                && raw.last() == Some(&b'\n'),
            UNAVAILABLE
        );
        Ok(raw[prefix.len()..raw.len() - 1].to_vec())
    }
}
pub(crate) fn read(
    git: &super::HostGit,
    root: &Path,
    head: &str,
    options: &Options,
    d: &Deadline,
) -> Result<History> {
    d.check()?;
    ensure!(root.is_absolute(), UNAVAILABLE);
    let root = root.canonicalize().context(UNAVAILABLE)?;
    walk(&mut GitObjects { git, root: &root }, head, options, d)
}

#[cfg(test)]
#[path = "git_history_read_tests.rs"]
mod tests;
