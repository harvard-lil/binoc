use std::process::Command;

fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn python_binding_tests() {
    if std::env::var_os("BINOC_RUN_PYTHON_BINDING_TESTS").is_none() {
        eprintln!("BINOC_RUN_PYTHON_BINDING_TESTS not set, skipping");
        return;
    }

    let root = workspace_root();
    let python_dir = root.join("binoc-python");

    assert!(
        python_dir.join("tests").exists(),
        "binoc-python/tests not found"
    );

    // Set up the virtualenv with dev dependencies (pytest + maturin).
    let sync = Command::new("uv")
        .args(["sync", "--extra", "dev"])
        .current_dir(&python_dir)
        .output()
        .expect("failed to run `uv sync --extra dev`");

    assert!(
        sync.status.success(),
        "uv sync failed:\n{}",
        String::from_utf8_lossy(&sync.stderr),
    );

    // Build the Python extension module into the virtualenv.
    let develop = Command::new("uv")
        .args(["run", "maturin", "develop"])
        .current_dir(&python_dir)
        .output()
        .expect("failed to run `uv run maturin develop`");

    assert!(
        develop.status.success(),
        "maturin develop failed:\n{}{}",
        String::from_utf8_lossy(&develop.stdout),
        String::from_utf8_lossy(&develop.stderr),
    );

    // Run the Python test suite.
    let pytest = Command::new("uv")
        .args(["run", "pytest", "tests", "-v"])
        .current_dir(&python_dir)
        .output()
        .expect("failed to run `uv run pytest tests -v`");

    let stdout = String::from_utf8_lossy(&pytest.stdout);
    let stderr = String::from_utf8_lossy(&pytest.stderr);
    if !stdout.is_empty() {
        eprintln!("{stdout}");
    }
    if !stderr.is_empty() {
        eprintln!("{stderr}");
    }
    assert!(pytest.status.success(), "Python binding tests failed");
}
