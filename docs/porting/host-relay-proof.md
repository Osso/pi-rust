# Host relay re-pin proof

Verified: 2026-10-21. Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, unchanged tracked files. Native commits: `a77b42e2` (whole modules/wiring), `f86b567e` (lossless text, sequence validation and lifecycle differentials), `3a95c8e8` (literal unchecked kind). [Contract](../specs/host-relay.md).

## Scope and command boundary

All four modules are implemented, with `main.rs`, `cli/args.rs`, core registration and a narrow lossless transaction adapter. Existing recipientHost producers/local-claim exclusion remain unchanged. The adapter preserves raw payload/kind and reuses canonical validation/collision/insertion; the global mailbox-kind enum is unchanged.

Every Cargo invocation used `CARGO_BUILD_JOBS=4` and `CARGO_TARGET_DIR=/home/osso/.worktrees/pi-rust-repin-host-relay/target`. Test binaries entered `.cargo/config.toml`'s mandatory sandbox runner. SSH fixtures are private executables; no real SSH/network/live state, push, merge, delegation, clippy or broad test suite was run.

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
| Whole workspace/types | `cargo check --workspace --all-targets` | `cargo-check-final.log`: exit 0, zero warnings; no new production code after this check |
| Formatting | `cargo fmt --check` before each commit | exit 0 |

All evidence paths are under `/tmp/pi-rust-repin-host-relay/`. Later documentation-only edits do not invalidate production proofs; the SQL-read test correction is compiled/passed by the exact focused command. Tests were not rerun merely for a commit milestone.

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

## Remaining boundaries

The four module implementations have no declared semantic deviation. Dedicated branch evidence remains absent for main-listener wake signal observation, injected stream failures, the 4096-unit stderr tail, long-connection reset, protocol-failure forced-kill timeout and resident timeout/lock-exhaustion cases; per-module traceability records these proof limits rather than inventing coverage.

Broader valid `--no-supervisor` orchestration/restart-handoff suppression remains main/CLI Q14, explicitly outside this relay slice. Frozen/native package versions are `0.80.3` / `0.84.1`; each resident compares against its own runtime config. Global package metadata was not changed. Native detached launch requires Linux `setsid`; [dependencies](../dependencies.md) records it. No independent verifier was delegated under the task's prohibition.
