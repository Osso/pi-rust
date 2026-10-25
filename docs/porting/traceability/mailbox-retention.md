# Mailbox retention

Frozen `packages/coding-agent/src/core/mailbox-retention.ts` at [4f9e6f8a4](../source-lock.json),8lines. Native: `crates/pi-coding-agent-storage/src/core/session_control_db/retention.rs`; no new runtime code was required. [Contract](../../specs/mailbox-retention.md).

| Frozen symbol/branch | Native mapping | Proof |
|---|---|---|
| `MAILBOX_MESSAGE_RETENTION_MS` | public same constant86400000, reexported by session_control_db | actual-source constant comparison on every case |
| `typeof createdAt !== 'string'` | exact first conversion `Option<JsString>` at raw ingress using `as_string`; None expires | missing/null/bool/number/array/object cases |
| invalid/nonfinite Date.parse | shared raw UTF-16 Date.parse authority returns finite time orNaN; native rejectsNaN | empty/invalid/lone-surrogate/out-of-range dates |
| valid date and `now - createdAtMs >= retention` | same f64 subtraction/comparison | epoch/prior/future/normalized dates; retention−1/exact/+1; negative/NaN/±Infinity clocks |

`repin_storage_retention::mailbox_retention_matches_frozen_unknown_ingress_and_numeric_boundaries` runs120 cases against the unmodified actual frozen module via `storage-retention-oracle.mts`. No mock, clock normalization, SQL approximation or default timestamp. Current helper already matched; this slice adds proof/mapping, not production changes. Log: `/tmp/pi-rust-repin-storage/mailbox-retention-matrix.log`.

No discrepancy or new semantic gap. Control-DB SQL retention cleanup, persistence and callsite ownership remain in their own module rows; this pure helper closure does not promote them.
