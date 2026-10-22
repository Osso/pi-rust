# SessionInfo Date values

Frozen `4f9e6f8a4`; [consumer/line mapping and proof](../porting/traceability/session-info-dates.md).

- Store created/modified as f64 Date time values: integral milliseconds or NaN for Invalid Date. Apply TimeClip at actual new Date(number) boundaries, never to raw message timestamps or accumulator numbers.
- Creation retains an invalid header date. Modification uses positive numeric last activity through TimeClip; otherwise use valid string header time or the filesystem/new-Date fallback, including pre-epoch time. Metadata Date.parse failures stay NaN, never epoch0.
- ISO conversion rejects Invalid Date at the frozen persistence boundary with RangeError/Invalid time value, before a metadata row is written. The temporary NaN-rejecting SessionInfo construction guard is removed.
- Date/NaN comparators use pinned V8 sort, not total_cmp or Rust's different stable-sort sequence. Preserve source named/literal/score priority and NaN-propagating subtree Math.max. Other consistent comparators remain unchanged.
- Picker/search/tree ordering, metadata-command ISO output and selector age rendering match frozen. Invalid Date deliberately renders NaNy as a recorded source discrepancy; no friendly-label correction or new fallback is introduced.
