# binoc-stat-binary

First-party optional Binoc plugin for statistical binary datasets.

The plugin reads Stata `.dta`, SAS `.sas7bdat`, and SAS transport `.xpt` files
into the standard tabular artifacts.

Stata `.dta` and SAS `.sas7bdat` files are single-dataset formats: each is its
own table, so the plugin publishes a leaf `tabular_v1` artifact (`headers` plus
string rows) directly on the file node, with no children.

A SAS transport `.xpt` file is a container that can hold several named datasets.
The plugin decomposes it into one `tabular_v1` child node per dataset, with no
artifact on the parent file node (which becomes a plain container, like an
archive). Each child uses the XPT dataset name as its stable logical name and
joins it to the file path with the `/>` decompose separator (for example
`study.xpt/>DM`). Duplicate dataset names are de-duplicated positionally.

It does not implement a bespoke diff; once a file is parsed, the shared tabular
writers and compaction rules report column, row, and cell changes on each table,
and membership changes (a dataset added, removed, or renamed) render as child
add/remove/move via the ordinary pair rules.

Cell values are flattened to strings for diffing. Stata and SAS missing values
are kept visible as source-format display tokens such as `.`, `.a`, and `.A`
rather than collapsed to empty strings.
