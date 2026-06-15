use std::path::PathBuf;

use binoc_stat_binary::test_support::{DtaMaterializer, Sas7bdatMaterializer, XptMaterializer};
use binoc_stdlib::test_vectors::{
    discover_vectors, run_vector_with_correspondence_engine_config, stdlib_materializers,
    VectorMaterializer,
};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-vectors")
}

#[test]
fn test_all_vectors() {
    let vectors = discover_vectors(&vectors_dir());
    assert!(
        !vectors.is_empty(),
        "No test vectors found at {}",
        vectors_dir().display()
    );

    let stdlib = stdlib_materializers();
    let dta = DtaMaterializer;
    let sas7bdat = Sas7bdatMaterializer;
    let xpt = XptMaterializer;
    let mut materializers: Vec<&dyn VectorMaterializer> = stdlib
        .iter()
        .map(|m| &**m as &dyn VectorMaterializer)
        .collect();
    materializers.push(&dta);
    materializers.push(&sas7bdat);
    materializers.push(&xpt);

    for vector in &vectors {
        run_vector_with_correspondence_engine_config(
            vector,
            &vectors_dir(),
            &materializers,
            |dataset| {
                let mut config =
                    binoc_stdlib::correspondence::engine_config_for_dataset_config(dataset);
                binoc_stat_binary::register_correspondence_rules(&mut config);
                config
            },
        );
    }
}
