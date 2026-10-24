//! Recover omitted imports of source-owned free declarations without renaming them.
use super::*;

impl ContextForRepair<'_> {
    pub(super) fn dependency_import(&self, repair: &Repair, d: &Deadline) -> Result<Vec<TextEdit>> {
        let Repair::SourceDependency {
            original_definition,
            current_definition,
            export_insertion,
            insertion,
            original_references,
            current_references,
            new_name,
            module_text,
        } = repair
        else {
            anyhow::bail!("Move source-dependency repair kind mismatch");
        };
        ensure!(
            original_definition.path == self.symbol.path
                && current_definition.path == self.symbol.path
                && original_definition.text == current_definition.text
                && insertion.path == self.destination
                && insertion.start_byte == insertion.end_byte
                && insertion.text.is_empty()
                && module_text.len() <= 4096,
            "Move source-dependency repair origin/insertion mismatch"
        );
        let module_name: String = serde_json::from_str(module_text)?;
        ensure!(
            serde_json::to_string(&module_name)? == *module_text
                && relative_module_candidates(self.destination, &self.symbol.path)
                    .contains(&module_name),
            "Move source-dependency module must be relative to its original captured source"
        );
        let original_tree = self.tree(original_definition, true, d)?;
        let current_tree = self.tree(current_definition, false, d)?;
        for (tree, contents, definition) in [
            (&original_tree, self.before, original_definition),
            (&current_tree, self.current, current_definition),
        ] {
            let selected =
                ast::named_statement(tree, &contents[&self.symbol.path], &definition.text, d)?
                    .context("Move source-dependency must own a top-level declaration")?;
            let name = selected
                .node
                .child_by_field_name("name")
                .context("Move source-dependency name absent")?;
            ensure!(
                name.start_byte() == definition.start_byte
                    && name.end_byte() == definition.end_byte
                    && matches!(name.kind(), "identifier" | "type_identifier"),
                "Move source-dependency definition anchor mismatch"
            );
        }
        let expected = [Target::Captured {
            span: original_definition.clone(),
        }];
        let mut changes = self.free_reference_changes(
            original_references,
            current_references,
            new_name,
            Some(&expected),
            d,
        )?;
        if let Some(export) = export_insertion {
            export.check(self.current)?;
            let dependency = ast::named_statement(
                &current_tree,
                &self.current[&self.symbol.path],
                &current_definition.text,
                d,
            )?
            .context("Move source-dependency current declaration absent")?;
            ensure!(
                export.path == self.symbol.path
                    && export.start_byte == export.end_byte
                    && export.text.is_empty()
                    && export.start_byte == dependency.statement.end_byte(),
                "Move source-dependency export insertion escapes its declaration boundary"
            );
            let line = if self.current[&self.symbol.path].contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            changes.push(TextEdit {
                path: export.path.clone(),
                start_byte: export.start_byte,
                end_byte: export.end_byte,
                text: format!("{line}export {{ {} }};{line}", original_definition.text),
            });
        }
        let target_tree = self.tree(insertion, false, d)?;
        let moved = ast::named_statement(
            &target_tree,
            &self.current[self.destination],
            &self.symbol.name,
            d,
        )?
        .context("Move source-dependency moved declaration absent")?;
        ensure!(
            insertion.start_byte == moved.statement.end_byte(),
            "Move source-dependency import insertion escapes selected declaration"
        );
        let line = if self.current[self.destination].contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        changes.push(TextEdit {
            path: insertion.path.clone(),
            start_byte: insertion.start_byte,
            end_byte: insertion.end_byte,
            text: format!(
                "{line}import {{ {} as {new_name} }} from {module_text};{line}",
                original_definition.text
            ),
        });
        changes.sort_by(|a, b| (&a.path, a.start_byte).cmp(&(&b.path, b.start_byte)));
        ensure!(
            changes
                .windows(2)
                .all(|p| p[0].path != p[1].path || p[0].end_byte <= p[1].start_byte),
            "Move source-dependency repair overlaps"
        );
        Ok(changes)
    }
}
