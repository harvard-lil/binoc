# binoc-stat-binary

First-party optional Binoc plugin for statistical binary datasets.

The plugin reads Stata `.dta`, SAS `.sas7bdat`, and SAS transport `.xpt` files
into the standard `tabular_v1` artifact (`headers` plus string rows). It does
not implement a bespoke diff. Once a file is parsed, the normal
`binoc.tabular_analyzer` transformer reports column, row, and cell changes.

Cell values are flattened to strings for diffing. Stata and SAS missing values
are kept visible as source-format display tokens such as `.`, `.a`, and `.A`
rather than collapsed to empty strings. Variable labels, formats, and available
value-label dictionaries are attached to node metadata for context, but value
labels do not replace cell values during comparison.
