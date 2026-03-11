use crate::ir::Migration;

/// Serialize a migration to JSON. This is the canonical migration format,
/// not an outputter — migrations are always JSON.
pub fn to_json(migration: &Migration) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(migration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::DiffNode;

    #[test]
    fn to_json_produces_valid_json_round_trips() {
        let migration = crate::ir::Migration::new(
            "v1",
            "v2",
            Some(DiffNode::new("modify", "file", "data.csv").with_tag("binoc.content-changed")),
        );
        let json = to_json(&migration).unwrap();
        let parsed: crate::ir::Migration = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from_snapshot, migration.from_snapshot);
        assert_eq!(parsed.to_snapshot, migration.to_snapshot);
        assert!(parsed.root.is_some());
        assert_eq!(parsed.root.as_ref().unwrap().path, "data.csv");
    }
}
