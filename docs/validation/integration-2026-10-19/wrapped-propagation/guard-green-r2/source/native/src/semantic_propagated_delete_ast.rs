//! Preserve every surviving syntax node across original-coordinate removals.
use super::CAPACITY;
use crate::capture::Deadline;
use anyhow::{Result, ensure};
use std::ops::Range;
use tree_sitter::Tree;

type NodeBinding = (u16, bool, usize, Option<String>, usize, usize);

struct Removed {
    spans: Vec<Range<usize>>,
    prefix: Vec<usize>,
}
impl Removed {
    fn new(spans: &[Range<usize>]) -> Result<Self> {
        let mut spans = spans.to_vec();
        spans.sort_unstable_by_key(|span| span.start);
        let mut prefix = vec![0usize];
        let mut previous = 0;
        for span in &spans {
            ensure!(
                span.start <= span.end && span.start >= previous,
                "Invalid propagated deletion ranges"
            );
            prefix.push(
                prefix
                    .last()
                    .unwrap()
                    .checked_add(span.len())
                    .ok_or_else(|| anyhow::anyhow!(CAPACITY))?,
            );
            previous = span.end;
        }
        Ok(Self { spans, prefix })
    }
    fn covers(&self, start: usize, end: usize) -> bool {
        let at = self.spans.partition_point(|span| span.start <= start);
        at > 0 && end <= self.spans[at - 1].end
    }
    fn position(&self, position: usize) -> usize {
        let at = self.spans.partition_point(|span| span.end <= position);
        let within = self
            .spans
            .get(at)
            .map_or(0, |span| position.saturating_sub(span.start));
        position - self.prefix[at] - within
    }
}

fn signature(
    tree: &Tree,
    removed: Option<&Removed>,
    visited: &mut usize,
    d: &Deadline,
) -> Result<Vec<NodeBinding>> {
    let mut result = Vec::new();
    let mut cursor = tree.root_node().walk();
    let mut depth = 0usize;
    loop {
        d.check()?;
        *visited += 1;
        ensure!(*visited <= 200_000, CAPACITY);
        let node = cursor.node();
        // The program survives even when every one of its children is removed.
        let skipped = node.parent().is_some()
            && removed.is_some_and(|removed| removed.covers(node.start_byte(), node.end_byte()));
        if !skipped {
            let position = |at| removed.map_or(at, |removed| removed.position(at));
            result.push((
                node.kind_id(),
                node.is_named(),
                depth,
                cursor.field_name().map(str::to_owned),
                // A program has no opening delimiter. Removing its first
                // child can leave original whitespace before its new AST
                // start; use the physical input start for this container.
                if node.parent().is_none() {
                    0
                } else {
                    position(node.start_byte())
                },
                position(node.end_byte()),
            ));
            if cursor.goto_first_child() {
                depth += 1;
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return Ok(result);
            }
            depth -= 1;
        }
    }
}

pub(super) fn unchanged(
    before: &Tree,
    after: &Tree,
    spans: &[Range<usize>],
    d: &Deadline,
) -> Result<()> {
    let removed = Removed::new(spans)?;
    let mut visited = 0;
    let original = signature(before, Some(&removed), &mut visited, d)?;
    let proposed = signature(after, None, &mut visited, d)?;
    ensure!(
        original == proposed,
        "Propagated deletion would alter surviving syntax"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removed_leading_declaration_keeps_surviving_node_roles_and_trivia() {
        let before = "export function removeMe() {}\nexport function keep() { return 2; }\n";
        let end = before.find('\n').unwrap();
        let after = &before[end..];
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&crate::analysis::grammar("typescript").unwrap())
            .unwrap();
        let old = parser.parse(before, None).unwrap();
        let new = parser.parse(after, None).unwrap();
        let d = Deadline::new(5_000);
        let result = unchanged(&old, &new, &[0..end], &d);
        if result.is_err() {
            let removed = Removed::new(&[0..end]).unwrap();
            let mut visited = 0;
            let a = signature(&old, Some(&removed), &mut visited, &d).unwrap();
            let b = signature(&new, None, &mut visited, &d).unwrap();
            eprintln!(
                "first-surviving-node-difference={:?}",
                a.iter().zip(&b).enumerate().find(|(_, (a, b))| a != b)
            );
        }
        result.unwrap();
        assert_eq!(after, "\nexport function keep() { return 2; }\n");
    }
}
