//! Original import attributes are inert source syntax, never host authority.
use super::*;
use tree_sitter::Node;

pub(super) fn suffix(node: Node<'_>, text: &str, d: &Deadline) -> Result<String> {
    let mut cursor = node.walk();
    let attributes: Vec<_> = node
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "import_attribute")
        .collect();
    ensure!(attributes.len() <= 1, "Move duplicate import attributes");
    let Some(attribute) = attributes.first() else {
        return Ok(String::new());
    };
    ensure!(
        attribute.end_byte() - attribute.start_byte() <= 65_536,
        "Move import attribute byte capacity"
    );
    let object = attribute
        .named_child(0)
        .context("Move attribute object absent")?;
    ensure!(
        object.kind() == "object" && attribute.child(0).is_some_and(|n| n.kind() == "with"),
        "Move attribute syntax unavailable"
    );
    let mut cursor = object.walk();
    let mut count = 0;
    for item in object.named_children(&mut cursor) {
        d.check()?;
        if item.kind() == "comment" {
            continue;
        }
        count += 1;
        ensure!(
            count <= 256
                && item.kind() == "pair"
                && item
                    .child_by_field_name("key")
                    .is_some_and(|n| matches!(n.kind(), "property_identifier" | "string"))
                && item
                    .child_by_field_name("value")
                    .is_some_and(|n| n.kind() == "string"),
            "Move attributes require bounded literal string pairs"
        );
    }
    Ok(format!(" {}", attribute.utf8_text(text.as_bytes())?))
}

pub(super) fn signature(
    language: &str,
    import: &Import,
    contents: &Contents,
    d: &Deadline,
) -> Result<Vec<String>> {
    import.span.check(contents)?;
    let tree = ast::parse(language, &import.span.text, d)?;
    let mut cursor = tree.root_node().walk();
    let nodes: Vec<_> = tree.root_node().named_children(&mut cursor).collect();
    ensure!(
        nodes.len() == 1 && matches!(nodes[0].kind(), "import_statement" | "export_statement"),
        "Move import attributes require one original linkage statement"
    );
    let suffix = suffix(nodes[0], &import.span.text, d)?;
    if suffix.is_empty() {
        return Ok(vec![]);
    }
    let mut cursor = nodes[0].walk();
    let attribute = nodes[0]
        .named_children(&mut cursor)
        .find(|n| n.kind() == "import_attribute")
        .context("Move attribute disappeared")?;
    let mut stack = vec![attribute];
    let mut result = Vec::new();
    let mut visits = 0;
    while let Some(node) = stack.pop() {
        d.check()?;
        visits += 1;
        ensure!(visits <= 4096, "Move import attribute syntax capacity");
        if node.child_count() == 0 || node.kind() == "comment" {
            result.push(format!(
                "{}:{}",
                node.kind(),
                node.utf8_text(import.span.text.as_bytes())?
            ));
        } else {
            for i in (0..node.child_count()).rev() {
                stack.push(node.child(i).context("Move attribute child absent")?);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "compiler_move_attribute_tests.rs"]
mod tests;
