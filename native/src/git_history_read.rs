//! Size-admitted commit-object traversal; independent of Git revision walkers.
use crate::capture::Deadline;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, VecDeque},
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
    committer_timestamp: u64,
    subject: String,
    body: String,
}
#[path = "git_history_cursor.rs"]
pub(crate) mod cursor;
fn committer_timestamp(value: &str) -> Result<u64> {
    let (_, metadata) = value.rsplit_once('>').context(UNAVAILABLE)?;
    let mut fields = metadata.split_ascii_whitespace();
    let seconds = fields.next().context(UNAVAILABLE)?;
    let timezone = fields.next().context(UNAVAILABLE)?;
    ensure!(
        fields.next().is_none()
            && timezone.len() == 5
            && matches!(timezone.as_bytes()[0], b'+' | b'-')
            && timezone.as_bytes()[1..].iter().all(u8::is_ascii_digit),
        UNAVAILABLE
    );
    let digits = seconds.strip_prefix('-').unwrap_or(seconds);
    ensure!(
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        UNAVAILABLE
    );
    let magnitude = digits.parse::<u64>().context(UNAVAILABLE)?;
    // Git uses an unsigned timestamp for its date-priority traversal, including
    // the unsigned conversion of a negative stored epoch. Author dates do not
    // determine this ordering. Malformed/overflowing timestamps refuse.
    Ok(if seconds.starts_with('-') {
        magnitude.wrapping_neg()
    } else {
        magnitude
    })
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
    let mut timestamp = None;
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
                timestamp = Some(committer_timestamp(value)?);
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
        committer_timestamp: timestamp.context(UNAVAILABLE)?,
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
    let mut frontier = VecDeque::from([(head.to_owned(), 0usize)]);
    // Pending IDs have no date authority until their bounded, hash-verified
    // object is read. Never choose among a partially admitted frontier.
    let mut ready = BTreeMap::<(Reverse<u64>, usize), (String, Commit)>::new();
    let mut seen = BTreeSet::from([head.to_owned()]);
    let (mut visited, mut bytes, mut merges, mut empty) = (0usize, 0usize, 0usize, 0usize);
    let mut objects_read = 0usize;
    let mut remaining_parents = options.max_parent_ids;
    let mut commits = Vec::new();
    let mut stopped = None;
    'walk: while !frontier.is_empty() || !ready.is_empty() {
        d.check()?;
        if commits.len() >= options.max_commits {
            stopped = Some("commit_limit");
            break;
        }
        while let Some((id, ordinal)) = frontier.pop_front() {
            d.check()?;
            if objects_read >= options.max_visits {
                frontier.push_front((id, ordinal));
                stopped = Some("visit_limit");
                break 'walk;
            }
            let size = objects.size(&id, d)?;
            d.check()?;
            ensure!(
                size <= options.max_commit_bytes
                    && size <= options.max_history_bytes.saturating_sub(bytes),
                Capacity
            );
            // Charge lookahead as well as emitted/merge objects. The selected
            // row limit cannot turn unverified dates into an ordering proof.
            bytes += size;
            let raw = objects.contents(&id, size, d)?;
            d.check()?;
            ensure!(
                raw.len() == size && super::object_id("commit", &raw, head.len()) == id,
                UNAVAILABLE
            );
            let parsed = parse(&raw, head.len(), &mut remaining_parents, d)?;
            objects_read += 1;
            ready.insert((Reverse(parsed.committer_timestamp), ordinal), (id, parsed));
        }
        let (_, (id, parsed)) = ready.pop_first().context(UNAVAILABLE)?;
        visited += 1;
        for parent in &parsed.parents {
            d.check()?;
            if !seen.contains(parent) {
                ensure!(
                    frontier.len() + ready.len() < options.max_frontier,
                    Capacity
                );
                let ordinal = seen.len();
                seen.insert(parent.clone());
                frontier.push_back((parent.clone(), ordinal));
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
        summary: json!({"selectionOrder":"commit_date_priority_parent_insertion_order","outputOrder":"reverse_selection_order",
        "headCommitId":head,"visitedCommits":visited,"selectedCommits":commits.len(),"rawCommitBytes":bytes,
        "objectsRead":objects_read,"bufferedCommits":ready.len(),
        "mergeCommitsSkipped":merges,"emptyMessagesSkipped":empty,
        "parentIdsObserved":options.max_parent_ids-remaining_parents,"frontierCommits":frontier.len()+ready.len(),
        "exhausted":frontier.is_empty()&&ready.is_empty(),"truncationReason":stopped,
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
