# Public compaction index number domain

Authority: frozen `agent-core/src/harness/compaction/compaction.ts:422–474`, common pin `4f9e6f8a4`.

## Requirements

- [x] Public find_turn_start_index/find_cut_point accept binary64 indices, not usize-only subsets.
- [x] Match JS loop conditions for NaN, infinities, fractional/negative/empty ranges; do not truncate or clamp before source dereferences.
- [x] Preserve returned first-kept index and turn index, including supplied negative zero; no-cut ranges can return nonfinite/fractional start indices.
- [x] Invalid dense-array dereferences return the frozen TypeError name/message rather than panic or protocol-normalized values.
- [x] Preserve the ignored keepRecentTokens parameter and existing bounded suffix selection; internal valid-range suffix calculations remain usize.

## Implementation and proof

[Compaction traceability](../porting/traceability/compaction.md). `compaction_index_domain` compares765 frozen/native cases over full, metadata-only and empty arrays. Number tags are observed before JSON output so NaN/null and -0/0 cannot conceal a mismatch. Existing31 focused tests and whole frozen compaction/branch matrix remain GREEN.

The internal prepareCompaction caller derives bounds from the actual entry slice; only the public boundary and its required callers change. Typed malformed entry objects and exotic JS array properties remain outside native typed input.
