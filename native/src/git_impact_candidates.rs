//! Pure candidate derivation from already admitted immutable source members.
use super::{Budget, CheckedSource, DetailOptions, capacity, observe_candidate_pair};
use crate::capture::Deadline;
use anyhow::Result;
use serde_json::{Value, json};
use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

const BYTE_CHECK_INTERVAL: usize = 4096;

struct ContentGroup<'a> {
    representative: &'a CheckedSource,
    lines: Vec<&'a str>,
    base_count: usize,
    head_count: usize,
}

struct Profile<'a> {
    source: &'a CheckedSource,
    group: usize,
}

pub(super) fn compare(
    deleted: &[&CheckedSource],
    added: &[&CheckedSource],
    options: &DetailOptions,
    budget: &mut Budget,
    deadline: &Deadline,
) -> Result<(Vec<Value>, Value)> {
    deadline.check()?;
    let pairs = deleted
        .len()
        .checked_mul(added.len())
        .ok_or_else(capacity)?;
    if pairs > options.max_candidate_pairs {
        return Err(capacity());
    }

    // Groups cache checked byte equivalence, not merely matching hash strings.
    // Repeated bindings to the parent's same checked Arc need no new byte scan.
    let mut groups = Vec::new();
    let mut by_hash = BTreeMap::new();
    let mut similarity_lines = 0usize;
    let base = preprocess(
        deleted,
        true,
        &mut groups,
        &mut by_hash,
        &mut similarity_lines,
        options.max_similarity_lines,
        deadline,
    )?;
    let head = preprocess(
        added,
        false,
        &mut groups,
        &mut by_hash,
        &mut similarity_lines,
        options.max_similarity_lines,
        deadline,
    )?;
    let base_lines = side_line_count(&base, &groups, deadline)?;
    let head_lines = side_line_count(&head, &groups, deadline)?;
    let work = added
        .len()
        .checked_mul(base_lines)
        .and_then(|left| {
            deleted
                .len()
                .checked_mul(head_lines)
                .and_then(|right| left.checked_add(right))
        })
        .ok_or_else(capacity)?;
    if work > options.max_similarity_work {
        return Err(capacity());
    }

    // All pair/line/work admissions above precede the first pair observation.
    let mut rows = Vec::new();
    let mut base_candidates = BTreeMap::<&str, usize>::new();
    let mut head_candidates = BTreeMap::<&str, usize>::new();
    let mut no_signal = 0usize;
    let mut below_threshold = 0usize;
    for before in &base {
        deadline.check()?;
        for after in &head {
            observe_candidate_pair(deadline)?;
            let before_group = &groups[before.group];
            let after_group = &groups[after.group];
            let m = before_group.lines.len();
            let n = after_group.lines.len();
            let identical = before.group == after.group;
            let (score, matched) = if identical {
                (10_000usize, m)
            } else {
                let denominator = m.checked_add(n).ok_or_else(capacity)?;
                if denominator == 0 {
                    no_signal = no_signal.checked_add(1).ok_or_else(capacity)?;
                    continue;
                }
                let matched = intersection(
                    before_group.lines.as_slice(),
                    after_group.lines.as_slice(),
                    deadline,
                )?;
                let score = matched.checked_mul(20_000).ok_or_else(capacity)? / denominator;
                if score < 6000 {
                    below_threshold = below_threshold.checked_add(1).ok_or_else(capacity)?;
                    continue;
                }
                (score, matched)
            };
            increment(&mut base_candidates, before.source.path.as_str())?;
            increment(&mut head_candidates, after.source.path.as_str())?;

            // A row has only scalar fields and the two scalar-only bindings.
            // Charge its array membership before cloning any of those fields.
            budget.charge(1)?;
            deadline.check()?;
            let kind = if identical {
                if before_group.base_count == 1 && before_group.head_count == 1 {
                    "unique_identical_content"
                } else {
                    "identical_content_candidate"
                }
            } else {
                "modified_content_candidate"
            };
            let row = json!({
                "base": before.source.binding(),
                "head": after.source.binding(),
                "kind": kind,
                "score": score,
                "baseLineCount": m,
                "headLineCount": n,
                "matchedLineCount": matched,
                "baseIdenticalCount": before_group.base_count,
                "headIdenticalCount": after_group.head_count,
                "baseCandidateCount": 0,
                "headCandidateCount": 0,
                "ambiguous": true,
                "identityEstablished": false,
            });
            // Zero counts and the shorter `true` boolean reserve a disjoint
            // lower bound on final row bytes. The parent's final bounded writer
            // checks eventual count widths and `false` with the whole result.
            budget.reserve_serialized(&row)?;
            deadline.check()?;
            rows.push(row);
        }
    }

    // Qualifying unequal partners also contribute to exact rows' ambiguity.
    // No second pair scan, greedy selection, or all-pairs score table is used.
    for row in &mut rows {
        deadline.check()?;
        let base_count = *base_candidates
            .get(candidate_path(row, "base")?)
            .ok_or_else(capacity)?;
        let head_count = *head_candidates
            .get(candidate_path(row, "head")?)
            .ok_or_else(capacity)?;
        row["baseCandidateCount"] = json!(base_count);
        row["headCandidateCount"] = json!(head_count);
        row["ambiguous"] = json!(base_count > 1 || head_count > 1);
    }
    fallible_sort_by(&mut rows, deadline, |left, right, deadline| {
        let base_order = compare_bytes(
            candidate_path(left, "base")?.as_bytes(),
            candidate_path(right, "base")?.as_bytes(),
            deadline,
        )?;
        if base_order != Ordering::Equal {
            return Ok(base_order);
        }
        let left_score = left["score"].as_u64().ok_or_else(capacity)?;
        let right_score = right["score"].as_u64().ok_or_else(capacity)?;
        let score_order = right_score.cmp(&left_score);
        if score_order != Ordering::Equal {
            return Ok(score_order);
        }
        compare_bytes(
            candidate_path(left, "head")?.as_bytes(),
            candidate_path(right, "head")?.as_bytes(),
            deadline,
        )
    })?;
    deadline.check()?;
    let summary = json!({
        "deletedCount": deleted.len(),
        "addedCount": added.len(),
        "pairsConsidered": pairs,
        "similarityWork": work,
        "similarityLines": similarity_lines,
        "qualifiedPairs": rows.len(),
        "noSignalPairs": no_signal,
        "belowThresholdPairs": below_threshold,
    });
    Ok((rows, summary))
}

fn preprocess<'a>(
    sources: &[&'a CheckedSource],
    base: bool,
    groups: &mut Vec<ContentGroup<'a>>,
    by_hash: &mut BTreeMap<&'a str, Vec<usize>>,
    total_lines: &mut usize,
    max_lines: usize,
    deadline: &Deadline,
) -> Result<Vec<Profile<'a>>> {
    let mut profiles = Vec::new();
    for &source in sources {
        deadline.check()?;
        let mut found = None;
        if let Some(indices) = by_hash.get(source.hash.as_str()) {
            for &index in indices {
                deadline.check()?;
                if same_bytes(source, groups[index].representative, deadline)? {
                    found = Some(index);
                    break;
                }
            }
        }
        let index = if let Some(index) = found {
            charge_lines(total_lines, groups[index].lines.len(), max_lines)?;
            index
        } else {
            let lines = normalized_lines(source.text.as_ref(), total_lines, max_lines, deadline)?;
            let index = groups.len();
            groups.push(ContentGroup {
                representative: source,
                lines,
                base_count: 0,
                head_count: 0,
            });
            by_hash.entry(source.hash.as_str()).or_default().push(index);
            index
        };
        let group = &mut groups[index];
        let side_count = if base {
            &mut group.base_count
        } else {
            &mut group.head_count
        };
        *side_count = side_count.checked_add(1).ok_or_else(capacity)?;
        profiles.push(Profile {
            source,
            group: index,
        });
    }
    deadline.check()?;
    Ok(profiles)
}

fn normalized_lines<'a>(
    text: &'a str,
    total_lines: &mut usize,
    max_lines: usize,
    deadline: &Deadline,
) -> Result<Vec<&'a str>> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        if index % BYTE_CHECK_INTERVAL == 0 {
            deadline.check()?;
        }
        if byte == b'\n' {
            let end = if index > start && bytes[index - 1] == b'\r' {
                index - 1
            } else {
                index
            };
            push_line(
                text,
                start,
                end,
                &mut lines,
                total_lines,
                max_lines,
                deadline,
            )?;
            start = index.checked_add(1).ok_or_else(capacity)?;
        }
    }
    if start < bytes.len() {
        // An unterminated final CR is data; only the LF case strips one CR.
        push_line(
            text,
            start,
            bytes.len(),
            &mut lines,
            total_lines,
            max_lines,
            deadline,
        )?;
    }
    fallible_sort_by(&mut lines, deadline, |left, right, deadline| {
        compare_bytes(left.as_bytes(), right.as_bytes(), deadline)
    })?;
    Ok(lines)
}

fn push_line<'a>(
    text: &'a str,
    mut start: usize,
    mut end: usize,
    lines: &mut Vec<&'a str>,
    total_lines: &mut usize,
    max_lines: usize,
    deadline: &Deadline,
) -> Result<()> {
    deadline.check()?;
    let bytes = text.as_bytes();
    while start < end && matches!(bytes[start], b' ' | b'\t') {
        if start.is_multiple_of(BYTE_CHECK_INTERVAL) {
            deadline.check()?;
        }
        start += 1;
    }
    while end > start && matches!(bytes[end - 1], b' ' | b'\t') {
        if end.is_multiple_of(BYTE_CHECK_INTERVAL) {
            deadline.check()?;
        }
        end -= 1;
    }
    if start < end {
        charge_lines(total_lines, 1, max_lines)?;
        // ASCII boundaries preserve UTF-8 boundaries in the admitted text.
        lines.push(&text[start..end]);
    }
    deadline.check()?;
    Ok(())
}

fn charge_lines(total: &mut usize, count: usize, max_lines: usize) -> Result<()> {
    let next = total.checked_add(count).ok_or_else(capacity)?;
    if next > max_lines {
        return Err(capacity());
    }
    *total = next;
    Ok(())
}

fn side_line_count(
    side: &[Profile<'_>],
    groups: &[ContentGroup<'_>],
    deadline: &Deadline,
) -> Result<usize> {
    let mut total = 0usize;
    for profile in side {
        deadline.check()?;
        total = total
            .checked_add(groups[profile.group].lines.len())
            .ok_or_else(capacity)?;
    }
    Ok(total)
}

fn same_bytes(left: &CheckedSource, right: &CheckedSource, deadline: &Deadline) -> Result<bool> {
    deadline.check()?;
    if left.hash != right.hash || left.text.len() != right.text.len() {
        return Ok(false);
    }
    if Arc::ptr_eq(&left.text, &right.text) {
        return Ok(true);
    }
    Ok(compare_bytes(left.text.as_bytes(), right.text.as_bytes(), deadline)? == Ordering::Equal)
}

fn intersection(before: &[&str], after: &[&str], deadline: &Deadline) -> Result<usize> {
    let (mut left, mut right, mut matched) = (0usize, 0usize, 0usize);
    while left < before.len() && right < after.len() {
        deadline.check()?;
        match compare_bytes(before[left].as_bytes(), after[right].as_bytes(), deadline)? {
            Ordering::Less => left += 1,
            Ordering::Greater => right += 1,
            Ordering::Equal => {
                matched = matched.checked_add(1).ok_or_else(capacity)?;
                left += 1;
                right += 1;
            }
        }
    }
    deadline.check()?;
    Ok(matched)
}

fn increment<'a>(counts: &mut BTreeMap<&'a str, usize>, path: &'a str) -> Result<()> {
    let count = counts.entry(path).or_default();
    *count = count.checked_add(1).ok_or_else(capacity)?;
    Ok(())
}

fn candidate_path<'a>(row: &'a Value, side: &str) -> Result<&'a str> {
    row.get(side)
        .and_then(|binding| binding.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(capacity)
}

fn compare_bytes(left: &[u8], right: &[u8], deadline: &Deadline) -> Result<Ordering> {
    deadline.check()?;
    for (left, right) in left
        .chunks(BYTE_CHECK_INTERVAL)
        .zip(right.chunks(BYTE_CHECK_INTERVAL))
    {
        deadline.check()?;
        let order = left.cmp(right);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    deadline.check()?;
    Ok(left.len().cmp(&right.len()))
}

// An in-place fallible heapsort lets every comparison observe cancellation,
// without a second line array or a comparator that swallows deadline errors.
fn fallible_sort_by<T>(
    values: &mut [T],
    deadline: &Deadline,
    mut compare: impl FnMut(&T, &T, &Deadline) -> Result<Ordering>,
) -> Result<()> {
    deadline.check()?;
    let len = values.len();
    for root in (0..len / 2).rev() {
        sift_down(values, root, len, deadline, &mut compare)?;
    }
    for end in (1..values.len()).rev() {
        deadline.check()?;
        values.swap(0, end);
        sift_down(values, 0, end, deadline, &mut compare)?;
    }
    deadline.check()?;
    Ok(())
}

fn sift_down<T>(
    values: &mut [T],
    mut root: usize,
    end: usize,
    deadline: &Deadline,
    compare: &mut impl FnMut(&T, &T, &Deadline) -> Result<Ordering>,
) -> Result<()> {
    loop {
        deadline.check()?;
        if root >= end / 2 {
            return Ok(());
        }
        let Some(mut child) = root.checked_mul(2).and_then(|index| index.checked_add(1)) else {
            return Err(capacity());
        };
        if child >= end {
            return Ok(());
        }
        if child + 1 < end
            && compare(&values[child], &values[child + 1], deadline)? == Ordering::Less
        {
            child += 1;
        }
        if compare(&values[root], &values[child], deadline)? != Ordering::Less {
            return Ok(());
        }
        values.swap(root, child);
        root = child;
    }
}
