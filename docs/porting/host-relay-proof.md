# Host relay re-pin proof

Verified: 2026-10-21. Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, unchanged tracked files. Native commits: `a77b42e2` (whole modules/wiring), `f86b567e` (lossless text, sequence validation and lifecycle differentials), `3a95c8e8` (literal unchecked kind). [Contract](../specs/host-relay.md).

## Scope and command boundary

All four modules are implemented, with `main.rs`, `cli/args.rs`, core registration and a narrow lossless transaction adapter. Existing recipientHost producers/local-claim exclusion remain unchanged. The adapter preserves raw payload/kind and reuses canonical validation/collision/insertion; the global mailbox-kind enum is unchanged.

Every Cargo invocation used `CARGO_BUILD_JOBS=4` and `CARGO_TARGET_DIR=/home/osso/.worktrees/pi-rust-repin-host-relay/target`. Test binaries entered `.cargo/config.toml`'s mandatory sandbox runner. SSH fixtures are private executables; no real SSH/network/live state, push or delegation. The initial port did not merge or run clippy/broad suites; the verifier follow-up below supersedes that gate state.

## RED and GREEN ledger

| Scope / revision | Command or case | Evidence |
|---|---|---|
| Before implementation, `166eac08` | `cargo test -p pi-coding-agent --test host_relay -- --nocapture` | `red.log`: real relay serve fails with No models available, proving wrong CLI dispatch |
| Protocol implementation | Bare-CR and distinct lone-surrogate peer/origin differential | `green-third.log` RED: bare CR treated as trailing JSON; fixed with Node-compatible line decoding and UTF-16 identity |
| Standalone CLI boundary | Exact standalone resident-console rejection test | `red-standalone.log` RED; early reject and frozen parser diagnostics now pass |
| Sequence allocation | Counter -1 allocates zero | `lifecycle-and-sequence-red.log` RED; source rolls back while initial native implementation accepted zero; final allocation validates both new/existing sequence |
| SSH spawn failure | Non-executable fake SSH in an otherwise SSH-free PATH | `red-spawn-green-seq.log` RED: native io display differs from `spawn ssh EACCES`; shared Node formatter fixes it |
| Text exports | Direct requireText/nullableText frozen differential | `red-text-validation.log` RED on surrogate replacement; final exports retain UTF-16 until explicit SQLite boundaries |
| Literal unchecked kind | Protocol `kind:"custom"` | `red-custom-kind.log` RED; native initially rejected source-valid wire input; narrow lossless adapter now persists literal kind |
| Implementation `f86b567e` | `cargo test -p pi-coding-agent --test host_relay --test host_relay_protocol --test host_relay_store --test host_relay_resident --no-fail-fast -- --nocapture` | `green-followup.log`: 20/20 pass, zero warnings |
| Final implementation `3a95c8e8` | Same focused command | `green-final.log`: 20 tests pass; added custom-kind test initially asserted the wrong runtime projection rather than canonical storage |
| Corrected custom-kind fixture, same production code | `cargo test -p pi-coding-agent --test host_relay_store custom_kind_survives_frozen_queue_native_import_and_frozen_read -- --exact --nocapture` | `green-custom-kind.log`: pass. Frozen SQLite reads literal `custom`; frozen runtime-message projection normalizes it, as native projection already does. Higher-sequence retry preserves original canonical bytes. Combined current focused proof: 21 passing tests, no ignores. |
| Final current fixture/code, `d9d18bbe` | Same four-target focused command after the canonical SQL-read correction | `green-complete.log`: exit 0, 21/21 pass, zero warnings, no ignores. This rerun followed the relevant test-source correction and also rebuilt the restored non-mutant binaries. |
| Whole workspace/types, `d9d18bbe` | `cargo check --workspace --all-targets` | `cargo-check-complete.log`: exit 0, zero warnings; no new code after this check |
| Formatting | `cargo fmt --check` before each commit | exit 0 |

All evidence paths are under `/tmp/pi-rust-repin-host-relay/`. Later documentation-only edits do not invalidate the final proofs. The failed custom-kind assertion was corrected to observe canonical bytes, not the frozen API's intentionally normalized projection; the four focused targets then passed together. Tests were not rerun merely for a commit milestone.

## Production mutants

Each mutant changed real production logic, ran one exact sandboxed behavioral test, failed by assertion (not compilation), and was restored immediately. `mutants.json` plus `mutant-*.log` retain full evidence.

| Mutant | Failing behavior |
|---|---|
| Inbound high-water `<=` → `<` | Deleted message resurrects after replay |
| Recipient-host route equality removed | Frozen error/ACK matrix diverges |
| Reads serialized behind blocked writer | Incoming mailbox cannot import before writer release |
| SSH alive-interval argv changed | Actual fake-executable argv differs |
| Resident version check removed | Old resident is incorrectly reused instead of replaced |
| Lossless expected kind reset to message | Higher-sequence custom-kind retry incorrectly conflicts |

First five target unchanged module branches at `f86b567e`; sixth covers the new raw-kind helper at `3a95c8e8`. Six of six killed. Sources restored byte-for-byte; no mutant is committed or installed.

## Verifier follow-up (2026-10-21)

Fix `02a4f2bc` addresses findings 1–4 from `/tmp/pi-rust-repin-host-relay-verify/REPORT.md`. Merge `45832c8f` incorporates canonical master `65c7acce0f62e91f1e94af88c2f722a40f3b9006`; only conflict was coverage status counts, recomputed from all 686 merged source rows. Test-only lint cleanup `1fdb3c10` removes redundant JsString conversions and an unpolled unused future exposed after library lint failures were removed. No production changes after `02a4f2bc`.

| Finding / boundary | Evidence |
|---|---|
| Original UTF-16 peer through canonical senderHost; replacement only at SQLite TEXT keys | `surrogate_peer_canonical_sender_host_matches_frozen`: exact canonical SQL bytes and both ACKs match frozen, cursor 2 and one row |
| Lossless expected routing at higher-sequence retry | `surrogate_routing_higher_sequence_retry_matches_frozen`: sender/recipient session and nullable agent IDs each retain lone surrogate through existing-row validation; exact frozen bytes/ACKs/cursor, no global enum change |
| Immediate EOF completion with blocked output; retained queued writes | `eof_completes_without_drain_and_retains_queued_output`: resolves within 200 ms before release, queued hello writes after return. Independent frozen blocked-output driver resolves immediately. Existing inbound-progress/queued-ACK differential still passes |
| Lint errors | Resident retry iterates sorted delays plus final attempt; canonical defaults use let-chain. Library and four relay test targets pass clippy with `-D warnings` |

All three new regressions were RED at `47628267`: corrupt canonical peer, routing retry conflict, EOF timeout (`fix-red.log`). All six protocol tests passed after fix (`fix-green.log`). Three production mutants independently restored peer lossiness, expected-address lossiness, and EOF drain waiting; all failed behavioral assertions, then sources restored byte-for-byte (`fix-mutants.json`, `fix-mutant-{peer,routing,eof}.log`). Merge left these production files unchanged.

Final commands use the same worktree, sandbox runner, four jobs and target directory as above. Full ledger: `fix-final-ledger.json` under `/tmp/pi-rust-repin-host-relay/`.

| Revision / command | Result / artifact |
|---|---|
| `45832c8f`: `cargo test -p pi-coding-agent --test host_relay --test host_relay_protocol --test host_relay_store --test host_relay_resident --test session_control_db --test agents_core_runtime --test approved_mailbox_deltas --no-fail-fast -- --nocapture` | 93 passed, no failures/ignores/warnings; seven private sandbox executions; `fix-final-tests.log` |
| `1fdb3c10`: rerun only changed `host_relay_protocol` and `host_relay_store` targets | 14 passed, no failures/ignores/warnings; other five targets unchanged since preceding proof; `fix-final-tests-followup.log` |
| `1fdb3c10`: `cargo check --workspace --all-targets` | Exit 0, zero warnings; `fix-final-check-followup.log` |
| `1fdb3c10`: `cargo clippy -p pi-coding-agent -j4 --lib --test host_relay --test host_relay_protocol --test host_relay_store --test host_relay_resident -- -D warnings` | Exit 0, zero warnings; `fix-final-clippy-followup.log` |
| `1fdb3c10`: `cargo fmt --all --check` | Exit 0; `fix-final-fmt-followup.log` |

Later proof-only edits do not invalidate this ledger. Frozen tracked source remains clean at the immutable common pin. Manual changed-Rust readability audit retains existing long orchestration/adapter functions noted by the verifier; analyzer unavailable, no unrelated redesign or suppression added. No independent delegation, push, canonical merge, real SSH/network or deployment.

## Remaining boundaries

The four module implementations have no declared semantic deviation. Dedicated branch evidence remains absent for main-listener wake signal observation, injected stream failures, the 4096-unit stderr tail, long-connection reset, protocol-failure forced-kill timeout and resident timeout/lock-exhaustion cases; per-module traceability records these proof limits rather than inventing coverage.

Broader valid `--no-supervisor` orchestration/restart-handoff suppression remains main/CLI Q14, explicitly outside this relay slice. Frozen/native package versions are `0.80.3` / `0.84.1`; each resident compares against its own runtime config. Global package metadata was not changed. Native detached launch requires Linux `setsid`; [dependencies](../dependencies.md) records it. No independent verifier was delegated under the task's prohibition.
