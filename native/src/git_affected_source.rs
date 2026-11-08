use super::index::Index;
use super::{Budget, Eligibility, SourceAccess};
use crate::model::Entity;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const LINE_BLOCK: usize = 1024;

struct CachedSource {
    text: String,
    // One count per fixed byte block bounds line-index memory independently
    // of the number of newlines in an admitted source.
    line_prefix: Vec<usize>,
}

impl CachedSource {
    fn new(bytes: Vec<u8>, budget: &Budget<'_>) -> Result<Self> {
        let text = String::from_utf8(bytes)?;
        let mut line_prefix = vec![1];
        let mut line = 1usize;
        for chunk in text.as_bytes().chunks(LINE_BLOCK) {
            budget.check()?;
            line += chunk.iter().filter(|byte| **byte == b'\n').count();
            line_prefix.push(line);
        }
        Ok(Self { text, line_prefix })
    }

    fn line(&self, offset: usize) -> usize {
        let block = offset / LINE_BLOCK;
        self.line_prefix[block]
            + self.text.as_bytes()[block * LINE_BLOCK..offset]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count()
    }

    fn range(&self, start: usize, end: usize) -> Result<&str> {
        ensure!(
            start <= end
                && end <= self.text.len()
                && self.text.is_char_boundary(start)
                && self.text.is_char_boundary(end),
            "Invalid affected UTF-8 source range"
        );
        Ok(&self.text[start..end])
    }

    fn entity(&self, entity: &Entity) -> Result<()> {
        self.range(entity.start_byte, entity.end_byte)?;
        ensure!(
            entity.start_line == self.line(entity.start_byte)
                && entity.end_line == self.line(entity.end_byte.saturating_sub(1)),
            "Invalid affected entity line binding"
        );
        match (entity.name_start_byte, entity.name_end_byte) {
            (None, None) => {}
            (Some(start), Some(end)) => {
                ensure!(
                    start >= entity.start_byte && end <= entity.end_byte,
                    "Affected name anchor outside entity"
                );
                ensure!(
                    self.range(start, end)? == entity.name,
                    "Affected name anchor spelling mismatch"
                );
            }
            _ => anyhow::bail!("Incomplete affected name anchor"),
        }
        Ok(())
    }
}

pub(super) struct SourceCache<'a> {
    access: &'a dyn SourceAccess,
    sources: BTreeMap<String, CachedSource>,
}

impl<'a> SourceCache<'a> {
    pub(super) fn new(access: &'a dyn SourceAccess) -> Self {
        Self {
            access,
            sources: BTreeMap::new(),
        }
    }

    fn source(
        &mut self,
        index: &Index<'_>,
        path: &str,
        budget: &mut Budget<'_>,
    ) -> Result<&CachedSource> {
        budget.check()?;
        let expected = *index
            .files
            .get(path)
            .context("Affected source outside snapshot")?;
        // A content-cache hit never supplies membership or policy authority for
        // a second side/path. Check that side's retained scope every time.
        let file = self.access.file(index.side, path)?;
        ensure!(
            file == expected,
            "Affected source manifest binding mismatch"
        );
        if !self.sources.contains_key(file.hash.as_str()) {
            budget.charge_source(file.size)?;
            let bytes = self.access.read_source(index.side, path)?;
            ensure!(bytes.len() == file.size, "Affected source size mismatch");
            let mut digest = Sha256::new();
            for chunk in bytes.chunks(65_536) {
                budget.check()?;
                digest.update(chunk);
            }
            ensure!(
                hex::encode(digest.finalize()) == file.hash,
                "Affected source hash mismatch"
            );
            let source = CachedSource::new(bytes, budget)?;
            self.sources.insert(file.hash.clone(), source);
        }
        let source = self
            .sources
            .get(file.hash.as_str())
            .context("Affected source cache entry missing")?;
        ensure!(
            source.text.len() == file.size,
            "Affected cached source size mismatch"
        );
        budget.check()?;
        Ok(source)
    }

    pub(super) fn validate_entity(
        &mut self,
        index: &Index<'_>,
        entity: usize,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        budget.check()?;
        let entity = index
            .graph
            .entities
            .get(entity)
            .context("Affected entity index missing")?;
        ensure!(
            entity.snapshot_id == index.snapshot.id,
            "Affected entity snapshot mismatch"
        );
        self.source(index, &entity.path, budget)?.entity(entity)?;
        if let Some(parent) = entity
            .parent_key
            .as_deref()
            .and_then(|key| index.keys.get(key))
        {
            let parent = &index.graph.entities[*parent];
            ensure!(
                parent.snapshot_id == entity.snapshot_id
                    && parent.path == entity.path
                    && parent.start_byte <= entity.start_byte
                    && entity.end_byte <= parent.end_byte,
                "Invalid affected direct parent binding"
            );
            // The direct parent contributes a source claim, but does not grant
            // a method-owner profile or require a recursive parent forest.
            self.source(index, &parent.path, budget)?.entity(parent)?;
        }
        budget.check()?;
        Ok(())
    }

    pub(super) fn eligibility(
        &mut self,
        index: &Index<'_>,
        entity: usize,
        budget: &mut Budget<'_>,
    ) -> Result<Eligibility> {
        self.validate_entity(index, entity, budget)?;
        let entity = &index.graph.entities[entity];
        if entity.language != "typescript" {
            return Ok(Eligibility::Unsupported("unsupported_language"));
        }
        if entity.kind != "method" {
            return Ok(Eligibility::Unsupported("unsupported_entity_kind"));
        }
        let Some(owner) = entity
            .parent_key
            .as_deref()
            .and_then(|key| index.keys.get(key))
            .copied()
        else {
            return Ok(Eligibility::Unsupported("missing_parent"));
        };
        let parent = &index.graph.entities[owner];
        if parent.language != "typescript" || !matches!(parent.kind.as_str(), "class" | "interface")
        {
            return Ok(Eligibility::Unsupported("unsupported_owner"));
        }
        if entity.name_start_byte.is_none() {
            return Ok(Eligibility::Unsupported("missing_name_anchor"));
        }
        if entity.name == "constructor" {
            return Ok(Eligibility::Unsupported("constructor_name_not_expanded"));
        }
        Ok(Eligibility::Expandable { owner })
    }

    pub(super) fn validate_evidence(
        &mut self,
        index: &Index<'_>,
        evidence: usize,
        budget: &mut Budget<'_>,
    ) -> Result<()> {
        budget.check()?;
        let evidence = index
            .graph
            .evidence
            .get(evidence)
            .context("Affected evidence index missing")?;
        let expected = *index
            .files
            .get(evidence.path.as_str())
            .context("Affected evidence outside snapshot")?;
        ensure!(
            evidence.snapshot_id == index.snapshot.id
                && evidence.content_hash == expected.hash
                && evidence.anchor == "exact",
            "Affected evidence source binding mismatch"
        );
        let source = self.source(index, &evidence.path, budget)?;
        source.range(evidence.start_byte, evidence.end_byte)?;
        ensure!(
            evidence.start_line == source.line(evidence.start_byte)
                && (evidence.end_line == source.line(evidence.end_byte)
                    || evidence.end_line == source.line(evidence.end_byte.saturating_sub(1))),
            "Invalid affected evidence line binding"
        );
        budget.check()?;
        Ok(())
    }

    pub(super) fn name_range(
        &mut self,
        index: &Index<'_>,
        entity: usize,
        budget: &mut Budget<'_>,
    ) -> Result<Option<(usize, usize, usize, usize)>> {
        self.validate_entity(index, entity, budget)?;
        let entity = &index.graph.entities[entity];
        let (Some(start), Some(end)) = (entity.name_start_byte, entity.name_end_byte) else {
            return Ok(None);
        };
        Ok(Some(self.range(index, &entity.path, start, end, budget)?))
    }

    pub(super) fn range(
        &mut self,
        index: &Index<'_>,
        path: &str,
        start: usize,
        end: usize,
        budget: &mut Budget<'_>,
    ) -> Result<(usize, usize, usize, usize)> {
        let source = self.source(index, path, budget)?;
        source.range(start, end)?;
        // Output projects canonical lines from the original byte offsets;
        // validation above still accepts both released evidence end-line forms.
        let end_position = if start == end { start } else { end - 1 };
        Ok((start, end, source.line(start), source.line(end_position)))
    }
}
