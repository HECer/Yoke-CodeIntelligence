//! Independently resolved actual transport catalog data; category API names only.
use super::{MAX_CONTEXT, Profile};
use crate::{capture::Deadline, model};
use anyhow::{Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct Context {
    pub variables: Map<String, Value>,
    pub evidence: Value,
}
fn names<'a>(catalog: &'a Value, d: &Deadline) -> Result<BTreeMap<String, &'a Value>> {
    let rows = catalog
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("session_prompt_catalog_invalid"))?;
    ensure!(rows.len() <= 256, "session_prompt_catalog_capacity");
    let mut result = BTreeMap::new();
    for row in rows {
        d.check()?;
        let name = row["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("session_prompt_tool_name_invalid"))?;
        ensure!(
            !name.is_empty()
                && name.len() <= 200
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "session_prompt_tool_name_invalid"
        );
        ensure!(
            result.insert(name.into(), row).is_none(),
            "session_prompt_tool_name_duplicate"
        );
    }
    Ok(result)
}
pub(crate) fn resolve(
    profile: Profile,
    full: &Value,
    selected: &Value,
    d: &Deadline,
) -> Result<Context> {
    let all = names(full, d)?;
    let actual = names(selected, d)?;
    ensure!(
        actual.iter().all(|(name, row)| all.get(name) == Some(row)),
        "session_prompt_selected_catalog_changed"
    );
    let registry: Value = serde_json::from_str(include_str!("session_prompt_markers.json"))
        .expect("Owned public category metadata");
    let mut markers = BTreeSet::new();
    if profile == Profile::Serena {
        for name in all.keys() {
            ensure!(
                registry["tools"].get(name).is_some(),
                "session_prompt_tool_markers_unavailable"
            );
        }
        for name in actual.keys() {
            for marker in registry["tools"][name]
                .as_array()
                .expect("Owned marker names")
            {
                markers.insert(marker.as_str().expect("Owned marker name").to_owned());
            }
        }
    } else if profile == Profile::Native {
        // Native aggregate entries have their own actual capability categories;
        // no unimplemented upstream tool or adapter is inferred from a label.
        if actual.contains_key("ci_find") {
            markers.insert("ToolMarkerSymbolicRead".into());
        }
        if actual.contains_key("ci_session") {
            markers.insert("ToolMarkerCanEdit".into());
        }
    }
    let mut tool_names = Map::new();
    for name in all.keys() {
        let canonical = match profile {
            Profile::Trail => name.strip_prefix("trail_").unwrap_or(name),
            Profile::Graft => name.strip_prefix("graft_").unwrap_or(name),
            _ => name,
        };
        ensure!(
            tool_names.insert(canonical.into(), json!(name)).is_none(),
            "session_prompt_tool_alias_ambiguous"
        );
    }
    let variables=json!({"available_tools":actual.keys().collect::<Vec<_>>(),"available_markers":markers.into_iter().collect::<Vec<_>>(),"tool_names":tool_names}).as_object().unwrap().clone();
    crate::json::encode_bounded(&variables, MAX_CONTEXT, d)?;
    Ok(Context {
        variables,
        evidence: json!({"profile":profile,"selectedCatalogSha256":model::hash(serde_json::to_vec(selected)?),"fullCatalogSha256":model::hash(serde_json::to_vec(full)?),"selectedToolCount":actual.len(),"markerMetadataRevision":registry["upstreamRevision"],"markerMetadataSha256":model::hash(include_bytes!("session_prompt_markers.json")),"markerScope":if profile==Profile::Serena {"pinned_public_tool_marker_api"}else{"own_transport_registered_categories"},"categoriesGrantAuthority":false,"complete":false}),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn category_metadata_covers_every_pinned_public_name_and_preserves_nonediting_symbolic_reads() {
        let d = Deadline::new(10000);
        let full = crate::compatibility::serena_tools();
        let selected = Value::Array(
            full.as_array()
                .unwrap()
                .iter()
                .filter(|row| row["name"] == "find_symbol")
                .cloned()
                .collect(),
        );
        let ctx = resolve(Profile::Serena, &full, &selected, &d).unwrap();
        assert_eq!(ctx.variables["available_tools"], json!(["find_symbol"]));
        assert_eq!(
            ctx.variables["available_markers"],
            json!(["ToolMarkerSymbolicRead"])
        );
        assert_eq!(ctx.variables["tool_names"]["find_symbol"], "find_symbol");
        resolve(Profile::Serena, &full, &full, &d).unwrap();
    }
    #[test]
    fn actual_prefix_mapping_preserves_canonical_names_and_refuses_forged_catalog_rows() {
        let d = Deadline::new(10000);
        let full = crate::trail_adapter::tools("graft");
        let selected = Value::Array(vec![full[0].clone()]);
        let ctx = resolve(Profile::Graft, &full, &selected, &d).unwrap();
        assert_eq!(ctx.variables["tool_names"]["find_code"], "graft_find_code");
        assert_eq!(ctx.variables["available_tools"], json!(["graft_find_code"]));
        let mut forged = selected.clone();
        forged[0]["description"] = json!("changed");
        assert!(resolve(Profile::Graft, &full, &forged, &d).is_err());
        let duplicate = Value::Array(vec![full[0].clone(), full[0].clone()]);
        assert!(resolve(Profile::Graft, &full, &duplicate, &d).is_err());
    }
}
