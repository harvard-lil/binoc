//! Materialize binoc-stat-binary's plugin test vectors.

use std::path::PathBuf;

use binoc_stat_binary::test_support::{DtaMaterializer, Sas7bdatMaterializer, XptMaterializer};
use binoc_stdlib::test_vectors::{
    discover_vectors, materialize_snapshots, stdlib_materializers, VectorMaterializer,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let output_root = PathBuf::from(args.next().unwrap_or_else(|| {
        "model-plugins/binoc-stat-binary/test-vectors-materialized".to_string()
    }));
    let vectors_root = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "model-plugins/binoc-stat-binary/test-vectors".to_string()),
    );

    let vectors = discover_vectors(&vectors_root);
    if vectors.is_empty() {
        eprintln!(
            "No vectors under {} (need manifest.toml + snapshot-a + snapshot-b).",
            vectors_root.display()
        );
        std::process::exit(1);
    }

    std::fs::create_dir_all(&output_root).expect("create_dir_all output_root");

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

    for vector in vectors {
        let name = vector
            .file_name()
            .expect("vector path has file name")
            .to_string_lossy();
        let dest = output_root.join(name.as_ref());
        eprintln!("{}", dest.display());
        materialize_snapshots(&vector, &dest, &materializers);
    }
}
