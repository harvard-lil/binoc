This vector commits real `.sas7bdat` files because the Rust `sas7bdat` crate
used by `binoc-stat-binary` is read-only and does not expose a writer API for
the CSV-style materializer pattern used by the Stata and XPT vectors.

The two files were sourced from the pandas SAS test corpus:

- `snapshot-a/data.sas7bdat`: `test1.sas7bdat`
- `snapshot-b/data.sas7bdat`: `test16.sas7bdat`

They share the same schema and row count but differ in many string cell values,
which makes the vector exercise real SAS7BDAT parsing and tabular cell-change
diffing without adding custom fixture-generation code that depends on an
external SAS writer.
