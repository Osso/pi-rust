# agent-harness.ts

Source: common-pin `agent-core/src/harness/agent-harness.ts:1–1030`.

| Capability | Implementation/proof | Status |
|---|---|---|
| Request-option copy/patch (`:59–117`) | `harness/stream_options.rs`, native header keys/values and arbitrary metadata; scalar/map Unchanged/Clear/Set; 3 behavioral RED→GREEN tests | Implemented, whole-harness differential pending |
| Provider stream acquisition rejection | Core StreamFn now returns Result; [38 low-level frozen scenarios and caller proof](stream-acquisition.md) | Harness catch/failure-content proof pending |
| Native request metadata/header-name forwarding | pi-ai metadata seam is now native; header names remain scalar under the parent-confirmed opaque-name rule, converted with rejecting `to_utf8()` at forwarding | Harness forwarding/unpaired-name test pending |
| Working prompt, skill/template, queues, hooks, failure/persistence, compaction/tree navigation, setters/abort/idle | Not yet implemented | Missing |

Removed old-only stream options `websocketConnectTimeoutMs` and `onRetry`; frozen curated options omit them. Header/metadata patches delete keys without moving retained keys, collapse patched empty maps to absent, preserve unpatched empty maps, and never mutate the base snapshot.

Owned-value getter implementation authorized provisionally by parent; broader user identity rule awaits approval. Frozen non-test consumer audit finds no built-in harness getModel/getResources callers; internal getResources calls only construct turn/resource-update snapshots. Evidence `/tmp/pi-rust-repin-delta-agent-core/harness-getter-consumers.txt`; frozen test asserts getModel identity, so ownership is not parity evidence.

No working-harness or whole-module completion claim. Session foundation: [tree session](agent-core-tree-session.md); low-level queues: [API re-pin](agent-core-api-repin.md).
