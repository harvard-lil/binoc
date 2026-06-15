//! Test-vector helpers for `binoc-excel`. Enabled by the `test-support`
//! feature. Provides [`ExcelMaterializer`], a [`VectorMaterializer`] that
//! builds a real `.xlsx` workbook from a `.xlsx.d` staging directory of `.csv`
//! files (one CSV per sheet, filename = sheet name). calamine is read-only, so
//! the writer side uses `rust_xlsxwriter`.

use std::path::{Path, PathBuf};

use binoc_stdlib::test_vectors::VectorMaterializer;
use rust_xlsxwriter::Workbook;

/// Builds `.xlsx` workbooks from staging directories of `.csv` files. Each CSV
/// becomes a worksheet named after the file stem; sheets are added in sorted
/// filename order. Numeric-looking cells are written as numbers so the typed
/// tabular model is exercised end-to-end.
pub struct ExcelMaterializer;

impl VectorMaterializer for ExcelMaterializer {
    fn suffixes(&self) -> &[&'static str] {
        &[".xlsx.d"]
    }

    fn build(&self, staging_dir: &Path, out_path: &Path, _all_staging_suffixes: &[&str]) {
        let mut csv_files: Vec<PathBuf> = std::fs::read_dir(staging_dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", staging_dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "csv"))
            .collect();
        csv_files.sort();

        let mut workbook = Workbook::new();
        for csv_path in &csv_files {
            let sheet_name = csv_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_else(|| panic!("non-utf8 sheet name in {}", csv_path.display()))
                .to_string();
            let contents = std::fs::read_to_string(csv_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", csv_path.display()));

            let worksheet = workbook.add_worksheet();
            worksheet
                .set_name(&sheet_name)
                .unwrap_or_else(|e| panic!("set sheet name {sheet_name:?}: {e}"));

            for (row_idx, line) in contents.lines().enumerate() {
                for (col_idx, field) in line.split(',').enumerate() {
                    let row = row_idx as u32;
                    let col = col_idx as u16;
                    let field = field.trim();
                    // First row is the header (always text); below it, write
                    // numeric-looking cells as real numbers.
                    if row_idx > 0 {
                        if let Ok(number) = field.parse::<f64>() {
                            worksheet
                                .write_number(row, col, number)
                                .unwrap_or_else(|e| panic!("write number: {e}"));
                            continue;
                        }
                    }
                    worksheet
                        .write_string(row, col, field)
                        .unwrap_or_else(|e| panic!("write string: {e}"));
                }
            }
        }

        if out_path.exists() {
            std::fs::remove_file(out_path)
                .unwrap_or_else(|e| panic!("remove_file {}: {e}", out_path.display()));
        }
        workbook
            .save(out_path)
            .unwrap_or_else(|e| panic!("save {}: {e}", out_path.display()));
    }
}
