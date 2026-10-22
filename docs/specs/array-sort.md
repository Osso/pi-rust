# Pinned comparator sort

[Primary source and proof](../porting/traceability/array-sort.md).

- `js_array_sort` sorts a dense typed slice using Node v26.11.1's V8 TimSort, preserving output identity order and comparator-call order even for inconsistent/stateful comparators.
- Accept f64 comparison results. NaN/±0 mean equal; negative/positive finite or infinite values use their sign. Preserve the short-array binary-insertion route, run detection/reversal, minrun, pending-run collapse, low/high merging, galloping and minGallop adaptation.
- Leave source values unmoved while comparison runs; apply the completed workarray order afterward. Generic T requires no Clone. Comparator panics are not caught.
- Use this helper only where the frozen comparator may be inconsistent, including Date NaN. Keep unrelated consistent comparator call sites on Rust stable sort. JS holes/accessors/default comparison/toSorted are outside the explicitly typed dense-slice API.
