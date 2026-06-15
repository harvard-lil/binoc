# binoc-stat-binary

First-party optional Binoc plugin for statistical binary datasets.

The plugin reads Stata `.dta`, SAS `.sas7bdat`, and SAS transport `.xpt` files
into the standard tabular artifacts. Ordinary single-table inputs publish
`tabular_v1` (`headers` plus string rows). Multi-member `.xpt` files publish a
`tabular_collection_v1` root plus one `tabular_v1` child per dataset member,
using the XPT dataset name as the stable logical table name and child path
suffix (for example `study.xpt::DM`). It does not implement a bespoke diff;
once a file is parsed, the shared tabular writers and compaction rules report
column, row, cell, and table-set changes.

Cell values are flattened to strings for diffing. Stata and SAS missing values
are kept visible as source-format display tokens such as `.`, `.a`, and `.A`
rather than collapsed to empty strings. Variable labels, formats, and available
value-label dictionaries are attached to node metadata for context, but value
labels do not replace cell values during comparison.

For `.xpt`, node metadata includes a `datasets` list so member inventory is
visible even when a file is empty or only one dataset is present.
