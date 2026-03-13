use binoc_sdk::Migration;

/// Serialize a migration to JSON.
pub fn to_json(migration: &Migration) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(migration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use binoc_sdk::DiffNode;

    #[test]
    fn to_json_produces_valid_json_round_trips() {
        let migration = Migration::new(
            "v1",
            "v2",
            Some(DiffNode::new("modify", "file", "data.csv").with_tag("binoc.content-changed")),
        );
        let json = to_json(&migration).unwrap();
        let parsed: Migration = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.from_snapshot, migration.from_snapshot);
        assert_eq!(parsed.to_snapshot, migration.to_snapshot);
        assert!(parsed.root.is_some());
        assert_eq!(parsed.root.as_ref().unwrap().path, "data.csv");
    }
}
