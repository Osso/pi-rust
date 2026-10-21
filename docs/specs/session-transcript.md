# Session transcript storage

Frozen target: 4f9e6f8a4. Reads resolve a missing `.jsonl` to its compressed sibling; an existing plain path wins. Inspection must never restore, replace, or migrate originals. Mutable archive opens fail before allocating an identity. Byte/text and bounded streaming reads propagate decompression failures.

Whole storage group remains pending spool/archive/history and manager integration proof.

Archives validate decompression before publication, preserve mode, create exclusive temporary sibling, rename then remove original; cleanup on failure. Metadata relocation and archive flag update use the control DB atomic operation. DB failure invokes reverse file operation; reverse failure replaces original error as in frozen TS. Restore prefers surviving plain file over stale archive.
