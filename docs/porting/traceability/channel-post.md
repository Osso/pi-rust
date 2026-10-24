# Channel post — common-pin map

Entire `packages/coding-agent/src/core/tools/channel-post.ts` at immutable `4f9e6f8a4`; native `core/tools/channel_post.rs`. [Contract](../../specs/channel-post.md). DB persistence/cursor helpers remain shared and unchanged.

## Symbol and branch matrix

| Frozen symbol/branch | Native | Proof |
|---|---|---|
| channelPostSchema; input/details; factory name/label/description/snippet/seven guidelines/sequential mode | Exact native schema and ToolDefinition | Complete enumerable definition differential in `differential_change_cwd_renderers::channel_post_complete_definition_matches_frozen`; retained Q5 tools-index metadata proof |
| execute child guard: nonempty agent ID OR required flag; absent context accepted until control-path requirement | Source truthiness, not Option presence | 7 execution/DB-state histories include ordinary/required/empty-ID controls; real RED empty-ID main rejected before f829a1b3 |
| requireControlDbPath: absent or empty rejected before message access | Native context getter with nonempty filter | 2 source/native path-before-message controls; real RED empty path reached message validation |
| message trim/nonempty check | Native JsString JS whitespace trim | ASCII/Unicode blank and raw-D800 message histories; complete source/native persisted body comparison at actual SQLite encoding boundary |
| sender nullable agent ID; session ID | RuntimeMailboxAddress, preserving empty-string ID distinct from None | Empty-ID success preserves sender and its exact cursor key; full persisted sender/body/ID state compared |
| postSharedChannelMessage then advanceSharedChannelCursor; failures reject; no enclosing transaction | Existing shared helpers in source order | 7 state histories; actual SQL-trigger post failure leaves no row/cursor, cursor failure leaves posted row but no cursor; same original caught error text |
| returned text/details | text_result and messageId | Whole source/native result comparison, including isError/terminate absence |
| renderCall creates/reuses Text, sets title | Existing fallible set_text_component and native theme | 11-frame source/native renderer history; title unchanged across args; narrow width, reuse/reset |
| renderResult text-only filter/join; error/output color; empty output | Existing exact render_tool_output plus fallible Text setter | Same11-frame history: mixed text/image, raw UTF16, error/empty output; both incompatible prior components throw original native TypeError |

Frozen ignores abort/update/tool-call arguments. Typed message inputs exclude missing/wrong runtime types not explicitly validated in source; no custom JavaScript getter/proxy emulation. Retained Q5 None-context tool matrix covers the absent-context error; live stale-context checks remain native context authority-owned.

## DB error audit

`channel-post-db-error-red.log` and `channel-post-db-error-red-2.log` are immediately GREEN falsifiers, not RED proof: non-directory parent, NUL/raw input and a raw-surrogate regular-file parent match frozen messages. No demonstrated propagation bug, so no speculative error adapter or DB change was added. Source filesystem/SQLite boundaries perform their own native encoding; arbitrary message/renderer units remain exact until those boundaries.

`channel-post-db-boundaries.log` is GREEN for both real SQLite-trigger failures. Full rows/sender/body/cursor are compared after rejection. Source's separate post/cursor statements preserve a successful post when cursor advancement fails; native does not silently retry or combine them into an invented transaction.

## Explicit runtime gap

| Case | Classification | Evidence |
|---|---|---|
| Persisted channel ID9007199254740992; next insert returns9007199254740993 | Node `node:sqlite` unsafe-lastInsertRowid binding gap, main ruling2026-10-24 under2026-10-23 runtime-oddity policy | `channel-post-large-id-red.log`: frozen rejects `Value is too large to be represented as a JavaScript number: 9007199254740993`, native succeeds. Not frozen TS logic; practically unreachable sequence size. No shared channel.rs edit or native restriction. Regression retained ignored with complete original failing assertions and explicit reason |

## Proof ledger

- f829a1b3: `channel-post-truthiness-red.log` behavioral RED2 before guard/path correction; `channel-post-truthiness-green.log` GREEN7 complete execution/state histories and2 path-order controls.
- `channel-post-render-red.log`: complete descriptor already GREEN, real renderer RED at incompatible-component frame8. 86ba5bdd routes existing fallible Text helper. `channel-post-render-green.log` GREEN4: channel descriptor/11frames plus retained cwd descriptor/11frames.
- Former cwd-only source oracle renamed to `coordination-render-sequence-oracle.ts`; factory name is required explicitly. Native test shares existing descriptor/history helpers instead of duplicating them. No alternate path or default factory remains. f47bcef3 contains the rename only because a removed-path staging error aborted staging; remaining edits committed immediately in86ba5bdd, whose exact code is covered by the GREEN log.
- Final runtime after parent6730c14c merge6e030bb7:4 active differential tests plus3 retained behavior tests GREEN;1 approved unsafe-rowid gap ignored with assertions retained (`channel-post-final-runtime.log`).
- Gate at08c1362c: workspace all-target keep-going JSON0errors/0warnings; warning-mode all-target/all-feature clippy exit0/global1686/changed-primary-spans0. Strict protected oracle compiler0, Biome0, fmt0; manual changed-line Rust/TS readability clean. InventoryPASS686/186679/446done/19gap/49partial/172todo. Logs `channel-post-{workspace,clippy,types,biome,inventory}*`.
- Whole gate/command/revision ledger: `/tmp/pi-rust-repin-core-tools/ledger.md`. Source/compiler fixtures are immutable pin imports; all state and databases are isolated test fixtures, not live channel posts.
