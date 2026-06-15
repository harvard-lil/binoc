use binoc_core::controller::Controller;

#[test]
fn column_add_remove_and_mid_row_insert_probe() {
    let temp = tempfile::tempdir().expect("temp dir");
    let left = temp.path().join("snapshot-a");
    let right = temp.path().join("snapshot-b");
    std::fs::create_dir_all(&left).expect("left dir");
    std::fs::create_dir_all(&right).expect("right dir");
    std::fs::write(
        left.join("data.csv"),
        "id,a,score\n1,alpha,10\n2,beta,20\n4,delta,40\n",
    )
    .expect("left csv");
    std::fs::write(
        right.join("data.csv"),
        "id,b,score\n1,alpha,10\n2,BETA,20\n3,gamma,30\n4,delta,40\n",
    )
    .expect("right csv");

    let controller = Controller::new(binoc_stdlib::correspondence::default_engine_config());
    let run = controller
        .diff_with_metrics(left.to_str().unwrap(), right.to_str().unwrap())
        .expect("diff succeeds");

    println!(
        "CHANGESET:\n{}",
        serde_json::to_string_pretty(&run.changeset).unwrap()
    );

    let root = run.changeset.root.as_ref().expect("root");
    let node = &root.children[0];
    let edits = node
        .details
        .get("edits")
        .and_then(|value| value.as_array())
        .expect("details.edits");
    let verbs: Vec<&str> = edits
        .iter()
        .filter_map(|edit| edit.get("verb").and_then(|verb| verb.as_str()))
        .collect();
    assert!(verbs.contains(&"tabular.rename_column"));
    assert!(!verbs.contains(&"tabular.add_column"));
    assert!(!verbs.contains(&"tabular.remove_column"));
    assert!(verbs.contains(&"tabular.edit_cell"));
    assert!(verbs.contains(&"tabular.add_row"));
}
