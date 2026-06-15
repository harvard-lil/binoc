use binoc_sdk::Edit;

pub fn value_size(value: &serde_json::Value) -> u64 {
    match value {
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => 1,
        serde_json::Value::String(value) => 1 + (value.len() as u64) / 16,
        serde_json::Value::Array(values) => 1 + values.iter().map(value_size).sum::<u64>(),
        serde_json::Value::Object(values) => {
            1 + values
                .iter()
                .map(|(_, value)| 1 + value_size(value))
                .sum::<u64>()
        }
    }
}

pub fn edit_cost(edit: &Edit) -> u64 {
    1 + value_size(&edit.params)
}

pub fn cost(edits: &[Edit]) -> u64 {
    edits.iter().map(edit_cost).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unknown_verbs_cost_the_same_as_known_ones() {
        let known = Edit::new("tabular.edit_cell", json!({"row": 1}));
        let unknown = Edit::new("custom.shift", json!({"row": 1}));
        assert_eq!(edit_cost(&known), edit_cost(&unknown));
    }

    #[test]
    fn fat_parameters_are_charged() {
        let thin = Edit::new("x.y", json!({"perm": [1, 0]}));
        let fat = Edit::new("x.y", json!({"perm": [1, 0], "blob": "a".repeat(160)}));
        assert!(edit_cost(&fat) > edit_cost(&thin) + 9);
    }
}
