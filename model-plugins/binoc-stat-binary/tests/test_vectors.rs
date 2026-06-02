use std::path::PathBuf;
use std::sync::Arc;

use binoc_sdk::test_support::{AbiComparator, AbiLogCollector};
use binoc_stat_binary::test_support::DtaMaterializer;
use binoc_stat_binary::{Sas7bdatComparator, StataComparator, XptComparator};
use binoc_stdlib::test_vectors::{
    abi_wrapped_default_registry, discover_vectors, run_vector_with_abi_log, stdlib_materializers,
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
    let mut materializers: Vec<&dyn VectorMaterializer> = stdlib
        .iter()
        .map(|m| &**m as &dyn VectorMaterializer)
        .collect();
    materializers.push(&dta);

    for vector in &vectors {
        let (mut registry, mut collectors, counter) = abi_wrapped_default_registry();

        let stata = Arc::new(AbiComparator::new(StataComparator, counter.clone()));
        collectors.push(stata.clone());
        registry
            .register_comparator(stata)
            .expect("same-build plugin");

        let sas7bdat = Arc::new(AbiComparator::new(Sas7bdatComparator, counter.clone()));
        collectors.push(sas7bdat.clone());
        registry
            .register_comparator(sas7bdat)
            .expect("same-build plugin");

        let xpt = Arc::new(AbiComparator::new(XptComparator, counter.clone()));
        collectors.push(xpt.clone());
        registry
            .register_comparator(xpt)
            .expect("same-build plugin");

        let collector_refs: Vec<&dyn AbiLogCollector> =
            collectors.iter().map(|c| c.as_ref()).collect();
        run_vector_with_abi_log(
            vector,
            &vectors_dir(),
            || {
                let mut direct = binoc_stdlib::default_registry();
                direct
                    .register_comparator(Arc::new(StataComparator))
                    .expect("same-build plugin");
                direct
                    .register_comparator(Arc::new(Sas7bdatComparator))
                    .expect("same-build plugin");
                direct
                    .register_comparator(Arc::new(XptComparator))
                    .expect("same-build plugin");
                direct
            },
            move || registry,
            &materializers,
            &collector_refs,
        );
    }
}
