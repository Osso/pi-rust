# `coding-agent/src/core/session-control-db.ts`

## Public mailbox reference normalization (2026-10-25)

Actual-source complete-wire RED `/tmp/storage3-control-file-refs-red.log` returns path/label references while native leaked original label/path order and unknown reference properties. The existing parse_file_refs/AgentFileReference serializer now supplies only the public projection; RuntimeMailboxMessage retains its existing lossless JsValue carrier. GREEN `/tmp/storage3-control-file-refs-green.log` compares the entire actual-source/native JSON string, including CJK filenames, and proves canonical SQLite bytes remain unchanged. This normalization follows frozen parseFileRefs; raw persistence still preserves its original fields/order.

## Retention-free reads under writer contention (2026-10-25)

Actual-source RED `/tmp/storage3-control-read-contention-valid-red.log` holds an owned SQLite BEGIN IMMEDIATE writer across both real-source and native reads. Frozen returns an empty mailbox; native's unconditional cleanup DELETE waits five seconds then fails database-locked. Mailbox/channel cleanup now performs the frozen due-row read before each conditional delete. GREEN `/tmp/storage3-control-read-contention-green.log` reads successfully while the writer remains held. Existing exact-age/invalid-birth differential remains GREEN in `/tmp/storage3-control-read-contention-clock.log`. This is ordinary WAL contention, not an injected OS fault or scheduling-race waiver. The first probe had a fixture-only JsString Display compile error and is not production evidence.

## Retained callback reentrancy (2026-10-25)

Owned-process RED `/tmp/storage3-control-reentrant-red.log` proves frozen nested withControlDb/enqueue returns ID1 while native deadlocks on its non-reentrant connection mutex. Main approved exact parking_lot for RetainedControlDb only; [dependency purpose](../../dependencies.md) and Cargo.lock are tracked. Its ReentrantMutex protects the existing RefCell/optional connection; nested callbacks borrow the same connection, while closing waits for the active-call count to reach zero. GREEN `/tmp/storage3-control-reentrant-green.log` also verifies an outer immediate transaction rolls back a nested public enqueue, leaving exactly the original message to claim. The child is bounded and killed/waited on failure; no unsafe pointer, alternate connection, fallback or production API is added.

## Runtime instance UUID wire format (2026-10-25)

Actual-source RED `/tmp/storage3-control-instance-red.log` validates frozen UUIDv4 then finds a native UUIDv7 incarnation in getRuntimeProcessInstanceId output. Generation now uses the existing random_uuid authority; its failures use the existing ControlDbError rather than panicking in the UUIDv7 generator. The public return type is unchanged. Source/native GREEN `/tmp/storage3-control-instance-green.log` checks cached wire equality, exact field order, UUID version/variant and the native live PID; three existing identity/cache tests pass in `/tmp/storage3-control-instance-unit.log`. Random values and process-specific identities are not falsely compared across runtimes.

## Empty-host mailbox wake (2026-10-25)

Actual-source process-boundary RED `/tmp/storage3-mailbox-notify-valid-red.log` receives the frozen SIGUSR2 wake but times out waiting for native enqueue with recipientHost:"". Both enqueue APIs now share the frozen absent/empty/local notification guard. The owned Node child installs its signal handler before registration and carries a recognized entrypoint argument; no provider or external process is used. GREEN `/tmp/storage3-mailbox-notify-green.log` observes real source, native stored-enqueue and native existing-row routing wakes. Retry identity remains under the existing approved empty-host delta.

## Runtime-registration probe rejection (2026-10-25)

Actual frozen baseline `/tmp/storage3-control-registration-source.log` preserves a coded TypeError from the injected live-process probe before replacement. Native API RED `/tmp/storage3-control-registration-api-red.log` accepted only bool. The existing registration option now returns Result<bool,CallbackError>; the existing ControlDbError source preserves it and aborts before listener/health mutation. The test compares full original error/call count and unchanged listener/health records. Default process probing is unchanged; no new carrier or callback family.

## Mailbox eligibility callback rejection (2026-10-25)

Actual frozen baseline `/tmp/storage3-control-eligibility-source.log` invokes the predicate twice, preserves its coded TypeError and leaves all three messages pending. Native API RED `/tmp/storage3-control-eligibility-api-red.log` could return only bool. Eligibility now returns Result<bool,CallbackError> through the existing ControlDbError callback source; failure aborts candidate collection before any claim. Tests throw at the first/second predicate and check exact original error/call count plus all unclaimed rows. Mechanical Ok adapters cover AgentSession, wait/question relay and existing fixtures; no outside-lane algorithm change or new carrier.

## Detached-finalization reference bytes and reciprocal replay (2026-10-25)

Actual-source RED `/tmp/storage3-control-finalization-red.log` shows new output references require label/path order while normalized existing references require path/label. Native used path/label for every reference, breaking canonical bytes and cross-runtime replay comparisons. The finalizer now constructs only its new output reference in the frozen order, leaving existing-reference normalization unchanged. The source/native fixture covers completed/failed/aborted outcomes, duplicate output-ref removal, complete stored bytes and TS-finalized→native-replay plus native-finalized→TS-replay.

## Superseded runtime recovery and replay (2026-10-25)

Actual-source RED `/tmp/storage3-control-recovery-red.log` exposes missing ordinary recovery branches: native rejected a live PID whose old incarnation was superseded by the registered supervisor, and rejected replay of an already recovered lost_runtime terminal row. Recovery now passes the replacement identity to the existing supersession authority and checks the frozen terminal/error/cleared-owner replay before active-runtime planning. Reconciliation supplies no replacement identity, preserving its existing behavior. Two source/native tests cover running→failed and cancelling→aborted, retained tool-call ID, unchanged replay result and exactly one terminal outbox row. Source creates both databases and primes replay through the real public API.

## Agent metadata clearing and insertion order (2026-10-25)

Actual-source RED `/tmp/storage3-control-agent-metadata-red.log` reproduces two ordinary failures: clearing lastActivity retained its previous value, and a combined activity/model update wrote model before currentActivity in canonical JSON. The activity helper now supplies the existing deletion sentinel for None; combined metadata is built in frozen currentActivity-then-model order. Source seeds both DBs; tests compare complete returned values and stored bytes. No new sentinel/carrier or metadata algorithm.

## Attached acquisition lifecycle and legacy owner guards (2026-10-25)

Actual-source terminal acquisition RED `/tmp/storage3-control-acquisition-red.log` returned invalid_agent for completed agents while native acquired ownership and advanced revision. The reader now propagates parsing failures and rejects non-recoverable lifecycles before ownership mutation, matching frozen ordering. Seven lifecycle states are tested with real registered supervisor processes and complete persisted agent/ownership comparisons.

Actual-source shared-DB RED `/tmp/storage3-control-legacy-owner-valid-red.log` preserves an alive nonce owner when its proposed replacement lacks a nonce; native incorrectly superseded it. Acquisition now reuses the existing runtime-process supersession authority (both nonces required); the duplicate weaker helper is deleted. The test uses TS registration/creation, native acquisition and unchanged persisted state. The first legacy probe had a fixture DTO comparison type error, not a production RED.

## Empty-parent creation guards (2026-10-25)

Actual-source attachment RED `/tmp/storage3-control-attachment-parent-red.log` treats an empty parent ID as main-thread attachment; native rejected it. Actual-source child RED `/tmp/storage3-control-child-parent-valid-red.log` rejects empty parent IDs before DB access, while native opened a blocked path and raised EEXIST. Attachments now use the frozen falsey-parent main alias; failed/runtime child creation returns parent_not_found before opening the DB. The first child probe failed only because its fixture supplied a nonexistent native input field; it is not production evidence.

## Detached-cancellation freshness rollback (2026-10-25)

Actual-source RED `/tmp/storage3-control-cancellation-real-red.log` shows expired cancellation timestamps advanced native lifecycle/revision while frozen rejected the command and rolled back the whole mutation. The lifecycle writer now calls the existing retention freshness authority inside the transaction, after the agent CAS and before command insertion, matching frozen ordering. The fixture compares original error, complete agent and canonical command bytes for expired/fresh cases. Earlier cancellation probe logs failed on fixture-only enum serialization/type inference/native-object JSON representation and are not production RED evidence.

## Raw agent writes versus store copies (2026-10-25)

Actual-source attachment creation RED `/tmp/storage3-control-agent-wire-red.log` shows typed agent serialization incorrectly applied store copyAccount order to a raw control-DB payload. Existing approved AgentNode order-only metadata now records object paths (including reference-array entries); no JSON values are duplicated. Raw control reads/creation/mutation retain original nested orders. Only copyAgent discards metadata for the objects it rebuilds (account root; result/reference, worker, transcript and event-stream subtrees), so store copies keep their distinct frozen constructor order. The fixture compares actual frozen/native attachment SQLite data bytes for five ordinary nested fields.

## Archive-candidate Unicode tie ordering (2026-10-25)

Actual-source RED `/tmp/storage3-control-archive-order-red.log` orders ordinary `𠮷野` and fullwidth `Ａ` filenames differently from native UTF-8 sorting. Candidate timestamps/paths now use the existing JsString SQL reader and UTF-16 ordering. The real-source fixture supplies Date to the exported selector; the native test covers date priority, equal-date filename order, cutoff equality, archived/subagent filtering and verifies selection does not archive rows. No new carrier or locale sort.

## Prompt-history migration return limits (2026-10-25)

Actual source/native RED `/tmp/storage3-control-history-red.log` shows ordinary negative migration limits returned every native legacy entry instead of frozen `slice(0, limit)`. Both prepared-empty and successful-insert returns now use the existing JS slice-index authority; persisted rows and subsequent SQLite LIMIT behavior remain unchanged. `repin_storage_control_history` covers six limits on four nonempty entries and four whitespace-only entries, comparing first return, complete stored rows and second return. Four entries prove the positive two-entry limit is not a single-page-only check. No new carrier or fallback. All12 source/native limit/history groups pass atde74f64e0 (`/tmp/storage3-control-history-green.log`); precommit workspace all-target check passes.

## Original callback throws (2026-10-25)

Actual frozen `withControlDb`/`withImmediateTransaction` baseline `/tmp/storage3-control-callback-source.log` preserves original throws and leaves the inserted row committed outside a transaction, rolled back inside one. API RED `/tmp/storage3-control-callback-api-red.log` could not represent CallbackError in ControlDbError. Main approved extending the existing shared SQLite error with one boxed callback source and From conversion, without new signatures or carriers. The existing CallbackError source walker restores original Error identity/properties and non-Error values; no new coercion or panic wrapper. The current eight-case fixture includes coded TypeError and retains existing null/string/object proof only. The eight-case source/native matrix passes at27f88577b (`/tmp/storage3-control-callback-green.log`); all-target workspace check and protected strict fixture types pass. The current60 control/public/differential/reciprocal/schema/concurrent tests also pass (`/tmp/storage3-audit-control_db.log`). Export correspondence audit finds all127 frozen exported functions represented by135 native public functions; this is correspondence, not behavioral acceptance.

## Reciprocal mailbox acceptance (2026-10-25)

`repin_storage_mailbox_compat` in `suite_core_4` proves TS enqueue → Rust claim/accept → TS read, then Rust enqueue → TS claim/accept → Rust reopen of the same recipient JSONL. Both receipts and messages survive, and canonical claims are deleted. RED `/tmp/storage2-mailbox-red.log` exposed a fixture ownership error, not a native delivery defect: frozen `deliverRuntimeMailboxMessage` rejects another process's claimant identity. SQLite reuses the deleted numeric ID, so the invalid second delivery made both final lookups see the latest row. Each runtime now registers/claims its own delivery; the frozen ownership check is unchanged. GREEN `/tmp/storage2-mailbox-green.log`: 1 differential passed. This is bounded compatibility evidence, not whole-module acceptance.

## Q15 writable-open filesystem failure

`interactive_login_defaults::authentication_completion_ordinary_defaults_match_frozen` uses real TS/Rust AgentSessions and persisted managers against a regular-file DB parent. RED `q15-auth-setter-failure-green.log` at `938d142a` is genuinely failing despite its filename: native rendered `File exists (os error 17)` rather than frozen EEXIST/mkdir/path text. `create_private_directory` now uses node_fs argument/error records; ControlDbError retains NativeNodeError and SessionManagerError retains the control error as source. AgentSession forwards that native error. Extended proof compares exact underlying error properties plus complete UI frames and the model mutation before failed metadata persistence. At `4f68b872`, all5 completion cases pass, including native error name/message/code/errno/syscall/path and full original frames; session_control_db46/46 passes and both-package all-target check has zero warnings. Grep found no other raw filesystem projections in session_control_db's open path.

## Q13 prompt-history SQLite read boundary

`read_prompt_history_rows` and `read_latest_prompt_history_entry` now request `JsString` through the existing shared Node SQLite `FromSql` implementation instead of requiring valid UTF-8 `String`. Actual frozen legacy migration uses lossless JSON and SQLite `json_extract`, so a migrated lone high unit stores ED A0 80; direct lone-unit string binding stores EF BF BD. Those two write boundaries remain unchanged. A real DB differential asserts complete stored hex bytes, first-load native units, later inserts and reopened history; the first migrated row is raw, exercising both latest-entry and full-history reads. RED was an actual rusqlite invalid-UTF-8 conversion error; shared `FromSql` makes source/native reopen and stored bytes equal. See [CustomEditor proof](interactive-custom-editor.md). This bounded correction does not establish whole control-DB parity.

## Q18 incoming content SQL boundary

`enqueue_incoming_message` now accepts native content; `IncomingControlMessage.content` and the claimed/delivered DTO retain `JsString`. SQLite's native `ToSql`/`FromSql` boundary matches Node TEXT encoding, rather than pre-repairing the input. `lossless_incoming_content` reproduces actual DTO rejection at `81c32624`, then compares native producer/claim and real stored content to the frozen source for isolated high/low units, a valid pair and NUL. Completion/failure transitions remain covered. Actual Cargo GREEN58 at `6759cfaa` across incoming content/error, control command, control differential and session-control DB (`incoming-content-incrate-green-2.log`), zero warnings. The temporary ASCII-only differential adapter is removed. Status tags and internal serialized identities remain scalar, not human content.

## Q18 incoming error SQL boundary

`fail_incoming_message` accepts `&JsString`; only its SQLite TEXT binding repairs unmatched units. `lossless_incoming_error` compares the same real source/native failure row, asserting source-equal stored text and unchanged native input. Actual Cargo GREEN1 at `37e55e61` (`native-control-boundaries-test.log`); this proves the incoming-error SQL boundary, not whole-module parity.

## Q18 single JSON authority slice

Stored agent/mailbox/request/schema JSON must use one `JsObject`/`JsValue` parser and lossless serializer, without a parallel scalar parsed object. `lossless_control_json_authority` adds source-written raw name/parentId/distinct-key metadata-patch byte preservation and native parent-request match/replacement-rejection regressions. Both pass through Cargo at `37e55e61` (`native-control-boundaries-test.log`, GREEN2). `mod.rs` now removes the scalar parsed-object helpers; validators/native string-field extraction and mailbox/agent projection consume the single lossless object. Agent metadata update/activity/model/slot/transcript APIs now accept/return `JsValue` and patch/stringify the original `JsObject`, preserving unrelated raw fields and key identity. Remaining agent persistence now reads native objects for lifecycle/terminal transitions, creation validation, descendant/parent-target lookup and dead-runtime recovery. Bootstrap/upsert validate the existing native payload directly instead of stringify→scalar-parse; canonical mailbox status now returns `Option<JsString>`. Detached finalization/replay now inspect and patch the same native agent object; supervisor response JSON and channel coordination classification serialize native values. Mailbox routing/delivery/claims, retention validation and legacy schema lifecycle/routing migration now read/mutate native objects directly; scalar mailbox-validation roundtrips are deleted. Claims serialize the same native object used for returned projection. Supervisor service and MCP permission response parsers were already native: audited without production edits. `lossless_control_response_parsers` records object/direct-string/text-content carriers with literal unpaired units and distinct argument keys (actual Cargo GREEN2 at `37e55e61`, `native-control-boundaries-test.log`). Native `session_control_db` fixtures pass 46 cases at `4cffa3fe`; store/lifecycle fixtures pass 23/23 each at `b2156192`. These bounded results are not whole-module acceptance.

Main decision (2026-10-22): process identity JSON is internal ASCII; not part of the lossless text authority. Preserve its existing serialized `String` carrier and SQL bindings; its escaped JSON representation is distinct from parsed human text/IDs. Main approved widening only the parser input to `impl Into<JsString>` so a claimant value read from native canonical JSON need not be scalarized; serializer/instance-ID/SQL carriers remain unchanged.

### Ordinary claim insertion order

Actual Cargo RED (`claim-order-incrate-red-2.log`, test fixture corrected in `c1f1f098`) showed missing `updatedAt` appended before `claimantProcessIdentity`. Frozen source `session-control-db.ts:916–922` appends `claimedAt, claimantProcessIdentity, status, updatedAt`; ordinary claims now follow that order while retaining existing key positions. `lossless_control_claim_order` claims the same source-created raw row with real source/native runtime listeners, restores the exact original row between claims, then compares canonical bytes after normalizing only volatile claim values. Actual Cargo GREEN1 at `642c0167` (`claim-order-incrate-green.log`). The earlier `callWire` double-decoding fixture failure is not production RED evidence.

## Canonical JSON counterexample — bounded regression now GREEN

The historical `489d4381` failure replaced a real TS-created body `before\ud800after` on public projection and claim. At `65e23c966`, the unchanged `repin_control_raw_json::canonical_mailbox_source_surrogate_body_survives_native_projection_and_claim` now passes against the actual frozen producer/reader (`/tmp/storage2-public-payload-final.log`): public projection and claim preserve the exact raw units. No production edit or waiver was required in this successor slice; the integrated native carriers already resolve this counterexample. This supersedes its former RED/blocker classification, not whole-module acceptance. Reciprocal public persistence8, migration4 and store-operation differential1 also pass at the same revision; [current bounded ledger](../storage-successor-proof.md).

### Newer bounded branch proofs through `056d2861`

These extend canonical persistence evidence, not public projection or whole-group acceptance.

Independent supervisor branch proof GREEN2: actual peer producers exercise approval→goal→advisory priority, exclusive in-flight claim, wrong-token requeue rejection, invalid response-kind rejection without mutation, rightful peer/native requeue/completion, relative deadline expiry and unexpired claim recovery, old-owner late-completion rejection, and completed-response stability across repeat recovery. Native-first initialization is covered in the recovery fixture. `supervisor-priority-recovery-proof.log`; no new production fix or raw-payload parity claim.

Legacy routing migration RED→GREEN2: historical claimed-routing merge now edits the original lossless canonical object, retaining raw body/distinct keys/creation/error strings while applying explicit legacy SQL routing/state fields. Both canonical CAS and migration writers now require original/raw serialized or JsObject authority; no serde-map writer remains. Delivered legacy rows still delete (`canonical-legacy-routing-green.log`). Public/store and legacy agent lifecycle migration boundaries remain open.

Legacy artifact migration RED→GREEN5: public-source-created raw mailbox body/unknown keys survive historical artifact cleanup, including array-contained artifact refs; only exact legacy keys disappear. Recursive payload migration now uses JsValue/JsObject/JsString for both mailbox/agent tables rather than normalizing unrelated values. Existing four migration fixtures still pass (`canonical-migration-raw-green.log`). Legacy routing/lifecycle writers and public store/projection boundaries remain separately open.

Agent writer RED2→GREEN9: source raw body/distinct keys survive native multi-agent pending→failed and atomic owned steering claim; canonical bytes for both now come from original lossless rows. Removed the unused serde-map CAS writer so no mutation caller can select it. Lifecycle rollback/replay/suppression8 remain passing (`canonical-agent-writers-green.log`); typed steering return payload remains a public projection blocker.

Enqueue identity RED→GREEN: source canonical U+D800 body and caller U+FFFD body previously aliased after lossy delivery-state stripping. Native now strips delivery keys from lossless JSON; both source/native reject the same collision without row mutation. Ordinary missing-birth retry remains idempotent (`canonical-enqueue-identity-green.log` GREEN2). Public projection/raw-input types remain pending.

Routing persistence RED→GREEN: a source-created unrouted message with raw surrogate body/distinct unknown keys is routed twice natively; source reads show exact original fields and routing idempotence. Routing writes modify only controlled routing fields in the original lossless canonical object. Public projection remains pending (`canonical-routing-raw-green.log`). Follow-up routing identity RED→GREEN: source raw U+D800 recipientSessionId must not alias a caller's U+FFFD recipient; source and native reject the same conflict without mutation. Routing assertions now compare canonical JsObject fields with strict UTF16 identity; typed producer fields only convert one way into JsValue. `canonical-routing-identity-green.log` GREEN2 covers rejection plus raw-field-preserving routing.

### Earlier canonical proof history

The following status/recovery evidence and root-cause description record the earlier checkpoint; newer bounded writer proofs above supersede its pending-writer inventory, not its public/store blockers.

Actual source SQL/receipt boundary: source callWire enqueue of storeRef `/fixture/\ud800/s` + messageId `\udfff` stores replacement-character SQL TEXT keys while canonical JSON `id` retains U+DFFF. A real source SessionManager receipt retains the original UTF16 refs. After verified owned-source-process exit, strict raw receipt identity does not equal row-derived SQL keys, so native recovery requeues rather than deletes. Recovery originally rewrote raw id/body/unknown keys (RED); its original-row lossless mutation now preserves them (GREEN1, `canonical-recovery-sql-green.log`). This proves row-derived returned SQL key encoding, not permission to normalize canonical/ref identity or every incoming identity interface.

Partial internal authority step: `canonical_status_mutations_preserve_source_utf16_body_and_distinct_unknown_keys` RED→GREEN preserves source U+D800 body, distinct U+D800/U+DFFF/U+FFFD unknown keys, and nested arrays through native claim/release/reclaim/fail. These mutations now parse/mutate/stringify the original lossless row and CAS its original serialized bytes. Public projection remains lossy and the combined public-read regression remains OPEN; routing/steering/store/migration writers still require the full lossless authority migration. This is not raw-regression or group acceptance.

Root: canonical `parse_stored_json_object` / `parse_json_object` still use legacy lossy serde `Value` maps, and runtime body is `String`. Lossless receipt scanning is fixed separately; it does not fix canonical payload authority or runtime payload egress. No typed-input exclusion or waiver applies to this source-created persisted JSON. Lossless public payload/egress interfaces require integration-owner coordination; no dual-authority fallback is added. Positive proofs below remain bounded, not whole-group/shared-state acceptance.

## Frozen 4f9e6f8a4 repin — bounded proof

Partial integration only; [integration status and open boundaries](../../specs/control-store-repin.md#integration-status--2026-10-21) are authoritative. The evidence below includes integrated control follow-ups through `2284dc6d`, not whole-module acceptance.

`repin_control_schema_constraints` adds behavior-only shared-schema proof with both primary initialization orders. Native and real frozen source schemas reject nested `artifactIds`/`artifactRefs` on insert and update in both agent/mailbox tables; failed writes leave original payload/timestamp/row count unchanged, and the other initializer preserves those rows. `schema-constraints-proof.log` GREEN1; no SQL-text/trigger-name assertion or whole-schema parity inference.

Whitespace pattern follow-up RED4→GREEN4 covers architect bodies, adjacent prompt-history dedup, legacy migration's original return versus trimmed stored rows, and terminal summary/error text. All native Rust trim sites in this module group now use the shared JS trim helper. Architect token renewal/completion, last-message roundtrip, sandbox identity/relocation/delete and unchanged base control rows have current bounded source-peer evidence (`whitespace-pattern-green.log`, `reciprocal-base-proof.log` GREEN8). These do not resolve the canonical JSON blocker above.

Channel trim RED→GREEN: actual source public posts distinguish JS U+FEFF/NBSP whitespace from U+0085 NEL. Native post now uses the shared JS trim helper; empty U+FEFF rejects before DB mutation. `channel-trim-green.log` focused1 pass over all three cases.

Notification-policy validation: `repin_control_validation` RED2→GREEN2 rejects null/string/number/array/object suppression values before DB setup and rejects invalid persisted policy at the restore boundary, matching TS errors. Typed input exclusions do not hide raw disk payloads.

Mutation negative control at `12acddd6`: disabling only expected-payload equality makes the real-peer-update CAS test fail at stale delivery rejection; restoring byte-identical mailbox source (SHA256 `72c1365c1b9e8d7ff7d9f243fd4b54b783523c439569113483c55695064d66f9`) makes it pass. Mutation never committed/deployed. All 127 frozen exported function names have native counterparts by source inventory; this is not 127 behavioral proofs.

Reciprocal proof: `repin_control_reciprocal.rs` GREEN6 starts fresh Rust public producers, then the same frozen TS initializes/reads, Rust mutates and the same TS reads again. Covers mailbox retry/claim/fail/delete, owned agent model/terminal/peer-ack replay, channel/cursor reserved-key coexistence, cancelled supervisor fencing, unchanged incoming/history/health/counters and archive failure after relocation rolling back metadata/counter moves.

`mailbox-order-cas-proof.log` GREEN13 adds actual TS upsert after a Rust claim/read snapshot (stale delivery CAS rejects), foreign claimant release/fail/delete fences, malformed complete receipt after an earlier matching ref aborts before deletion, and raw null-ref short-circuit ordering. Two tests reproduce TS discrepancies only, not accepted Rust contracts: limit0 and empty optional recipientHost retry.

Empty-host discrepancy pending classification: source first enqueue with `recipientHost:""` omits host/routes locally, but identical retry conflicts because insertion uses truthiness and identity uses nullish defaulting. Rust empty-host behavior is not claimed compatible; no silent waiver or fix. Ordinary nonempty/local host paths are independently proved.

Migration/liveness follow-up: RED delivered legacy outbox row survives→fixed v12 copy excludes delivered; RED legacy live process identity without an incarnation is wrongly superseded→fixed nonce supersession requires both incarnations, matching the exact TS helper. `migration-live-green.log` GREEN23 across exports/lifecycle/mailbox/migrations. Explicit fixture mutations start from public-TS-created primary schema: legacy secondary routing/outbox marker, metadata missing parent/obsolete search column, monotonic legacy counters, nested artifact stripping and owned-live-owner migration refusal followed by dead-owner success. Owned Node-child exit is awaited explicitly after tsx group termination; prior wrapper wait alone was not sufficient proof of child death.

`repin_control_retention_clock` passes real TS-created SQLite state through the actual Rust retention module at a controlled relative clock, comparing frozen TS cleanup at the same clock. Exactly24h and invalid/missing/null/numeric births expire; 24h−1ms remains for mailbox/channel. Rust rows are asserted before the same TS process reloads them; no peer-row normalization or runtime clock flags. Primary schemas are never hand-built.

`repin_control_exports.rs` compile-RED→GREEN4 supplies omitted frozen exports: resume metadata returns None only for an empty metadata index (Some(empty) after filters), cwd/literal-prefix/archive selection, empty-main candidates; peer-global restriction recovery claims; known-local metadata/health admission; live agent IDs over a real owned TS process lifetime; transaction-owned enqueue rolls back an ID collision and exposes only committed rows. Resume/empty queries cover both initialization orders. Exposes source `withControlDb` / `withImmediateTransaction` and transaction enqueue / notify-recipient primitives required by the later host-relay store; no relay transport is implemented here.

`repin_control_lifecycle.rs` RED4→GREEN7: direct steering consumes its canonical row and returns original payload/status; take-owned-steer atomically advances agent + claims row; an AFTER-agent-update fixture mutation forces message CAS failure and rolls back both changes; suppressed detached finalization emits no transport; finalization replay survives TS outbox deletion. Added missing cancellation suppression input (compile-RED): same-lifecycle suppression increments revision without a duplicate cancel command. Owned TS child process death exercises dead-runtime suppression with a real Rust supervisor identity/listener. No fabricated process IDs. Canonical delivery snapshot retains raw JSON extras; source store copyMessage deliberately drops them.

Retained cleanup / null birth: `repin_control_channel::retained_connection_sweeps_channel_on_source_minute_timer` RED→GREEN proves a retained Rust connection sweeps TS-mutated old channel rows on the real source 60s interval without reopen (60.65s targeted batch). Null-birth validation/defaulted retry RED→GREEN; same TS/Rust unknown delivered-status projection is `failed`, and senderHost survives projection. `retained-null-green.log`: 10 targeted tests pass.

Discrepancy pending user classification: frozen TS `takeRuntimeMailboxMessagesForDelivery(...,limit=0)` claims both rows in a fresh two-row fixture. `frozen_source_limit_zero_discrepancy_fixture` reproduces actual TS only; it is not approval to preserve or fix the Rust branch. Positive-limit contract proof continues independently.

`repin_control_agents.rs` RED3→GREEN3: TS-created mailbox birth remains immutable through Rust upsert; successful message consumption deletes peer row; TS-created owned child → Rust terminal commit → TS outbox delete → identical Rust replay succeeds; Rust outbox acknowledgement deletes and repeat acknowledgement returns false. Outbox delivery signature now matches `(db,record)` (removed obsolete timestamp), status excludes delivered, cleanup targets poisoned only. Mailbox failure mutations use payload CAS and creation cutoff; state/status reads filter birth retention. Expanded stale-failure/migration/suppression/finalization proof still pending.

`repin_control_mailbox.rs` RED5→GREEN5: real TS missing-birth enqueue → Rust idempotent retry/collision rejection; take yields claimed, stale payload cannot delete, release/reclaim then CAS-delete is visible to same TS; foreign-host/session-ID collision cannot claim/consume; creation-age expiration ignores fresh updates; dead owned TS claimant with a complete receipt is deleted rather than requeued on Rust registration. `repin_runtime_mailbox_transcript.rs` RED→GREEN proves raw persisted refs (including `[{}]`), null/malformed complete-record errors, ignored partial final JSON and inactive-branch receipt ordering. Shared [receipt scanner](runtime-mailbox-transcript.md) is the only recovery reader.

Source runtime status is pending/claimed/failed; unknown statuses project failed, not throw. `senderHost` and recipient host routing fields are represented. `markRuntimeMailboxMessageDelivered` retains its source export but now host-guards and CAS-deletes; removed `consumeRuntimeMailboxMessage` had no production caller. `takeRuntimeMailboxMessagesForDelivery` claims only; acceptance/fsync callers and expanded reverse-init/migration/retention-boundary proof remain pending. No SQLite-fixture crash-safety claim.

`repin_control_channel.rs` reproduces and resolves dropped origins/self-filter and initialization retention: imported same-session origins remain pending, origin fields survive TS/Rust projection, cursor decreases do not regress peer state, old relative-age TS-created rows expire on Rust initialization. Retained-connection timer proof is recorded above. `repin_control_requests.rs` reproduces cancelled→completed misprojection; resolves cancellation read, sender-fenced pending/claimed cancellation, repeat/wrong-sender rejection and token-fenced late completion. Three targeted tests pass.


`repin_control_sessions.rs` proves real TS-created v15 → Rust read/write → same TS read/reopen, and fresh Rust → TS initialization → Rust write → TS read. Removed `all_messages_text` from schema, public metadata and producer snapshots. NULL name/explicit clear survives peer writes. Candidate archive selection does not mutate rows. Descendant/subagent exports match plain/archived parent aliases; combined relocation/archive and relocation/unarchive commit together. Bulk metadata removal matches source.

`repin_control_schema.rs` proves Rust-first default INSERT birth, origin uniqueness and restriction-request uniqueness before TS initialization. Four focused tests pass; no whole-module claim. Retention-boundary proof, legacy payload migration proof, remaining mailbox/lifecycle/store deltas and failure-injected atomicity proof remain pending. Existing proofs below are historical, not renewed against the frozen pin. Central coverage remains unchanged.


Source: `packages/coding-agent/src/core/session-control-db.ts` (7047 lines) at the pinned revision.
Rust: `crates/pi-coding-agent-storage/src/core/session_control_db/` (`mod.rs`, `schema.rs`, `sessions.rs`,
`incoming.rs`, `mailbox.rs`, `requests.rs`, `channel.rs`, `health.rs`, `agents.rs`, `lifecycle.rs`).

The shared `control.sqlite` is opened by TypeScript and Rust processes at the same time. The historical inventory below maps intended contracts and earlier tests; it does not establish frozen-pin equivalence of every SQL statement, migration or transaction boundary.

## Frozen persisted DDL refresh (2026-10-21)

`cross_implementation_control_db::schema_text_and_pragmas_match_the_typescript_module` reproduced eight persisted `sqlite_master.sql` differences (50 objects on both sides): creation-age index, parent/empty-session indexes, channel creation/origin indexes, restriction-claim table, terminal-outbox table comment and default-birth trigger. Rust DDL strings now copy frozen bytes, including whitespace/comments (`session-control-db.ts` initialization and `MAILBOX_CREATED_AT_MS_SQL`). Fresh-schema cross-implementation tests pass all four cases, including byte-identical schema/PRAGMAs. A comment-only DDL mutant fails the exact schema assertion; restored. Existing DB definitions are not rewritten, matching source IF NOT EXISTS behavior. This is bounded persisted schema parity, not new SQL behavior or whole-module acceptance. Fresh fixture clocks and deleted-row/outbox assertions in `session_control_db` and `terminal_outbox_delivery` follow frozen creation-age expiry and canonical deletion.

## Schema and connection

| Contract | Rust | Proof |
|---|---|---|
| `CONTROL_DB_SCHEMA_VERSION = 15` | `CONTROL_DB_SCHEMA_VERSION` | `schema_and_pragmas_match_the_shared_contract`, `legacy_database_migrates_to_schema_fifteen`, `unsupported_future_schema_is_rejected` |
| `CREATE TABLE`/`CREATE INDEX` for all 17 tables | `schema.rs::CREATE_SCHEMA_SQL` | `cross_implementation_control_db::schema_text_and_pragmas_match_the_typescript_module` |
| Migration from legacy counters, payloads, runtime owners, outbox, mailbox, lifecycle rows, architect/listener/session-metadata columns | `initialize_schema` + `migrate_*` | `legacy_database_migrates_to_schema_fifteen` |
| Legacy artifact-field rejection triggers | `create_legacy_artifact_field_triggers` | migration test; trigger names asserted by cross-language schema test |
| `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000` | `configure_shared_sqlite_database` | `schema_and_pragmas_match_the_shared_contract`, cross-language schema test |
| `getControlDbPath(directory = getUserStateRoot())` | `get_control_db_path(&Path)` | `control_db_path_joins_control_sqlite` |
| `getRuntimeProcessInstanceId()` | `get_runtime_process_instance_id()` | listener registration/claim tests |
| `retainControlDbConnection()` | `retain_control_db_connection()` + `RetainedControlDbGuard` | `retained_connection_survives_and_releases` |

Every write runs inside `BEGIN IMMEDIATE` (`with_immediate_transaction`), matching TS; reads use a
short-lived or retained connection. `ControlDbError` carries the SQLite contention flag used by the
TS `isSqliteContentionError` retry branches.

## Exported symbols by area

All names below are re-exported from `pi_coding_agent::core::session_control_db` under the
snake_case form of the TypeScript name; the area's test file is `tests/session_control_db.rs`
unless noted.

| Area | TypeScript exports | Rust functions | Tests |
|---|---|---|---|
| Incoming messages | `enqueueIncomingMessage`, `claimLatestIncomingMessage`, `completeIncomingMessage`, `failIncomingMessage`, `readIncomingMessageStatus`, `IncomingControlMessage` | `incoming.rs` | `incoming_claim_returns_none_when_queue_is_empty`, `incoming_messages_claim_supersedes_pending_rows` |
| Runtime mailbox | `enqueueRuntimeMailboxMessage`, `enqueueStoredRuntimeMailboxMessage`, `recoverDeadRuntimeMailboxClaims`, `takeRuntimeMailboxMessagesForDelivery`, `claimRuntimeMailboxMessages`, `readRuntimeMailboxMessageForDelivery`, `consumeRuntimeMailboxMessage`, `deliverRuntimeMailboxMessage`, `consumeRuntimeMailboxMessageByStoreRef`, `markRuntimeMailboxMessageDelivered`, `releaseRuntimeMailboxMessageClaim`, `failRuntimeMailboxMessage`, `readRuntimeMailboxMessage`, `listRuntimeMailboxMessages` | `mailbox.rs` | `runtime_mailbox_routing_preserves_payload_order_and_identity`, `runtime_mailbox_claim_respects_registered_listener_pid`, `runtime_mailbox_claim_deliver_and_consume`, `mailbox_status_transitions_require_pending_state` |
| Listener registration | `registerRuntimeMailboxListener`, `retireRuntimeMailboxListener`, `readRuntimeMailboxListener`, `listRuntimeMailboxListeners`, `assertMainSessionRuntimeAvailable`, `resolveOwnMainRuntimeCoordinationRecipient`, `RuntimeMailboxListener`, `RuntimeMailboxRegistrationOptions`, `RuntimeMailboxAddress`, `RuntimeMailboxStoreRef` | `mailbox.rs` | `session_health_and_listener_registration_track_generations`, `runtime_mailbox_claim_respects_registered_listener_pid` |
| Architect requests | `postArchitectRequest`, `listPendingArchitectRequests`, `claimPendingArchitectRequests`, `renewArchitectRequestClaims`, `completeArchitectRequest`, `ArchitectRequest*` | `requests.rs` | `architect_requests_claim_renew_and_complete` |
| Supervisor requests | `postSupervisorRequest`, `readSupervisorRequest`, `claimNextSupervisorRequest`, `recoverSupervisorRequests`, `hasPendingSupervisorApprovalRequest`, `requeueSupervisorRequest`, `completeSupervisorRequest`, `Supervisor*` | `requests.rs` | `supervisor_requests_prioritize_approvals_and_validate_responses`, `supervisor_recovery_completes_expired_requests`, `supervisor_advisory_claim_requires_matching_response_kind` |
| Shared channel | `postSharedChannelMessage`, `listSharedChannelMessagesAfter`, `readSharedChannelCursor`, `hasPendingRuntimeCoordinationMessage`, `isRuntimeCoordinationMailboxMessage`, `initializeSharedChannelCursorAtTail`, `advanceSharedChannelCursor`, `readSharedChannelTail`, `SharedChannelMessage`, `PostSharedChannelMessageInput` | `channel.rs` | `shared_channel_posts_and_cursors`; differential `empty_channel_body_error_matches` |
| Session health | `readSessionHealth`, `listSessionHealth`, `writeSessionHealth` | `health.rs` | `health_record_round_trips_all_statuses` |
| Session metadata | `writeSessionMetadata`, `readSessionMetadata`, `listSessionMetadata`, `listActiveSessionMetadata`, `listArchivedSessionMetadata`, `findActiveSessionMetadataById/Name`, `removeSessionMetadata`, `archiveSession`, `unarchiveSession`, `archiveSessionsOlderThan`, `SessionMetadata`, `WritableSessionMetadata` | `sessions.rs` | `session_metadata_round_trips_and_archives`, `session_metadata_write_preserves_archived_state_and_index_flag` |
| Named sessions / relocation | `normalizeSessionName`, `writeSessionName`, `readSessionNameState`, `relocateSessionControlData`, `SessionNameState` | `sessions.rs` | `named_sessions_and_relocation_move_all_control_rows` |
| Last message / prompt history | `writeLastMessage`, `readLastMessage`, `recordPromptHistoryEntry`, `readPromptHistory`, `readOrMigratePromptHistory`, `LastControlMessage` | `sessions.rs` | `last_message_only_returns_assistant_roles`, `prompt_history_deduplicates_and_migrates_legacy_entries` |
| Goal / model / thinking / sandbox | `readSessionGoal`, `writeSessionGoal`, `writeSessionModel`, `writeSessionThinkingLevel`, `readSessionSandboxProfile`, `writeSessionSandboxProfile`, `clearSessionSandboxProfile` | `sessions.rs` | `session_metadata_identity_is_enforced_for_sandbox_profiles` |
| Multi-agent persistence | `bootstrapMultiAgentAgent`, `readMultiAgentAgent(s)`, `readMultiAgentState`, `upsertMultiAgentMailboxMessage`, `getMultiAgentMailboxMessageStatus`, `markMultiAgentMailboxMessageDelivered/Failed`, `writeMultiAgentCounters`, `allocateMultiAgentCounter`, `updateMultiAgentAgentActivity`, `updateMultiAgentAgentCurrentActivity`, `updateMultiAgentAgentSlot`, `updateMultiAgentAgentTranscript`, `MultiAgent*` records | `agents.rs` | `read_multi_agent_state_orders_rows_by_rowid`, `multi_agent_counters_allocate_monotonic_sequences`, `bootstrap_refuses_to_mutate_owned_rows` |
| Lifecycle mutations | `createMultiAgentChildWithRuntimeOwnership`, `createMultiAgentAttachment`, `createFailedMultiAgentChild`, `acquireAttachedRuntimeOwnership`, `readMultiAgentRuntimeOwnership`, `commitMultiAgentLifecycleMutation`, `commitMultiAgentSteeringMutation`, `commitMultiAgentSteeringDelivery`, `commitMultiAgentDetachMark`, `commitMultiAgentTerminalMutation`, `recoverDeadMultiAgentRuntime`, `reconcileDeadDetachedAgentRuntimes`, `MultiAgentRuntimeOwnership*`, `MultiAgentRuntimeOwnerIdentity`, `SupervisorRuntimeOwnership` | `agents.rs` | `child_creation_requires_active_parent_and_running_revision_one`, `detach_mark_is_idempotent_and_fenced_by_ownership`, `steering_mutations_require_registered_authority`, `lifecycle_and_terminal_mutations_write_the_outbox`, `agent_error_type_is_persisted_through_terminal_mutation` |
| Terminal outbox | `claimMultiAgentTerminalOutbox`, `deliverMultiAgentTerminalOutbox`, `failMultiAgentTerminalOutbox`, `cleanupMultiAgentTerminalOutbox`, `MultiAgentTerminalOutboxRecord`, `ClaimMultiAgentTerminalOutboxOptions` | `agents.rs` | `outbox_claim_recovers_stale_claims_and_poisons_after_max_attempts`, `terminal_outbox_replay_is_idempotent` |
| Detached artifacts / finalization | `listDetachedArtifactAgentsUpdatedAtOrBefore`, `finalizeDetachedJob`, `prepareControlDbForSelfRestart`, `FinalizeDetachedJob*`, `DetachedArtifactAgentRecord` | `agents.rs`, `lifecycle.rs` | `detached_artifact_listing_filters_by_label_and_lifecycle`, `finalize_detached_job_records_terminal_transport_and_artifacts` |

The session-manager consumer contract (20 symbols) is compile-locked by
`tests/control_db_public_api.rs::session_manager_contract_is_public`.

## Cross-implementation and differential proof

`tests/cross_implementation_control_db.rs` (temp DBs only):

- `typescript_writes_are_readable_by_rust`
- `rust_writes_are_readable_by_typescript_in_source_order` (v15 reference)
- `concurrent_typescript_and_rust_writers_keep_every_row`
- `schema_text_and_pragmas_match_the_typescript_module`

`tests/differential_control_db.rs` runs 74 operation steps against
`oracle/control-db-oracle.ts` on separate temp databases and compares `JSON.stringify` output
(key order included) after masking timestamps. Covers schema creation, session metadata, named
sessions, prompt history, incoming messages, mailbox routing/claims, architect/supervisor queues,
shared channel cursors, health, multi-agent bootstrap/state/lifecycle/outbox/counters, and
relocation.

## Discrepancies / gaps

- TypeScript's dual driver (`bun:sqlite` / `node:sqlite`) collapses to `rusqlite::Connection`; the
  observable schema, SQL, and PRAGMA contract is unchanged.
- `getRuntimeProcessInstanceId`: TS throws when `/proc/self/stat` cannot be read. Rust returns
  `Err(ControlDbError)` with the identity-read message instead of synthesizing a pid/zero-start-time
  identity; the value is memoized once, matching the `globalThis` cache. Proof:
  `process_instance_id_propagates_identity_read_failure`,
  `process_instance_id_serializes_identity_in_typescript_key_order`,
  `runtime_process_instance_id_uses_the_live_process_identity`.
- Typed-core exclusion: TS accepts arbitrary runtime payloads for `enqueueStoredRuntimeMailboxMessage`
  and mailbox JSON; Rust requires typed structs. Invalid JS payload shapes cannot be constructed.

## Mailbox payload key order

Every canonical mailbox payload mutation spreads the stored object and then appends keys in the
TypeScript literal order (`claimedAt`, `claimantProcessIdentity`, `status`, `updatedAt`; routing
appends `recipientSessionId`, `recipientAgentId`, `senderSessionId`, `senderAgentId`; delivery appends
`deliveredAt` then `error`). Rust inserts in the same order with `shift_remove`/`remove` after the
inserts, so `JSON.stringify` bytes match. Covered by the 74-step `differential_control_db` suite.

## Session-name v15 deviation (approved 2026-10-20)

Reference: `c248b0067c9fc3fa438f967f826c222875c54834` in
[source-lock.json](../source-lock.json); v15 is now included in the common re-pin (old approval to re-validate against 4f9e6f8a4).

| Contract / branch | Rust | Behavioral proof |
|---|---|---|
| Lifecycle target 14, database target 15; future-version error | `schema::{LIFECYCLE_PROTOCOL_SCHEMA_VERSION, CONTROL_DB_SCHEMA_VERSION}` | `legacy_database_migrates_to_schema_fifteen`, `unsupported_future_schema_is_rejected` |
| Legacy normalization/precedence, matching rows only, drop table, idempotence | `migrate_legacy_session_names`, `move_legacy_session_names_into_metadata` | `version_fourteen_session_names_migrate_into_metadata_and_drop_the_legacy_table` |
| Refuse live lifecycle owners before moving names; retry after owners stop | `assert_control_db_migration_quiescent` | `session_name_migration_requires_runtime_quiescence` |
| Absent/NULL versus stored empty name; missing-metadata writes throw | `read_session_name_state`, `write_session_name` | `session_names_live_only_in_session_metadata`, `write_session_name_requires_a_metadata_row` |
| Metadata snapshots preserve names and explicit clears; relocation moves name | `read_session_metadata_write_values`, `relocate_session_control_data` | `generic_metadata_snapshots_preserve_dedicated_session_names`, `named_sessions_and_relocation_move_all_control_rows` |

`cross_implementation_control_db` now uses the authorized v15 reference: exact schema/PRAGMA
comparison (no removed fields), TS-created already-v15 DB → Rust update → TS readback,
Rust-created DB → TS reads, and concurrent TS/Rust writers. All DBs are temporary.
`differential_control_db` retains all operation comparisons, replacing deleted TS named-session
APIs with `writeSessionName`/`readSessionNameState`. No v14 version expectation is masked.
The writer-lock migration-state race remains enforced in code; no deterministic race harness yet.

Quiescence errors preserve TS Set encounter order (runtime PIDs, then owner identities), not numeric sorting; `name_migration_errors_preserve_owner_pid_encounter_order` reproduces the prior Rust ordering failure with live parent/current process identities.

Existing v15 databases with live lifecycle owners accept name updates without rerunning lifecycle migration or mutating running-agent payloads: `existing_v15_names_update_without_reentering_lifecycle_migrations`. Schema comparison initializes separate TS/Rust databases and compares every schema/PRAGMA field.

## Approved delivery delta 8fbfcef2 (2026-10-21)

`approved_mailbox_deltas` compares the guarded, hash-locked delta against native temporary databases: limit zero leaves two rows pending; limit one claims exactly one. Frozen limit zero is asserted exactly as two claimed deliveries, an approved discrepancy, not parity. RED: native delivered 2 instead of 0 (`/tmp/pi-rust-repin-delta-proofs/limit-red.log`). GREEN/mutant results recorded in the lane report. OAuth excluded.

## Approved empty-host delta 93b7d9af

`approved_mailbox_deltas` proves source-first empty-host enqueue then native retry, native-first same-ID/no-extra-row retry, and genuine other-host conflict. Frozen retry asserts the exact canonical-routing Error. Native self-retry initially passed for the wrong reason (stored empty host); source-first RED exposed the routing mismatch. Match TS falsy omission on write and falsy-to-local comparison on retry.

Bounded mutation proof: restoring the post-delivery limit check, removing empty-host comparison normalization, or persisting empty recipientHost each independently fails the focused approved-delta behavior test (3 killed mutants; exact diffs/logs `/tmp/pi-rust-repin-delta-proofs/mutants.json`). Existing `repin_control_mailbox` frozen discrepancy fixtures now assert exact claims/error and successful native counterparts. No whole-control acceptance claim.
