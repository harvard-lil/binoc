//! Materialize workspace test vectors (built zips/tars) for local inspection.
//! Plugins that ship their own vectors provide their own materialize binary
//! composing [`stdlib_materializers`] with their plugin-specific builders —
//! see `binoc-sqlite/src/bin/materialize_test_vectors.rs`.

use std::path::PathBuf;

use binoc_stdlib::test_vectors::{
    discover_vectors, materialize_snapshots, stdlib_materializers, VectorMaterializer,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let output_root = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "test-vectors-materialized".to_string()),
    );
    let vectors_root = PathBuf::from(args.next().unwrap_or_else(|| "test-vectors".to_string()));

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
    let materializers: Vec<&dyn VectorMaterializer> = stdlib.iter().map(|m| &**m).collect();

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
