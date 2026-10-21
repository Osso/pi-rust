# Phase 0 re-pin infrastructure

Frozen source: `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` ([lock](source-lock.json)); infra branch `repin/infra`. Phase 0 changes infrastructure only. No product behavioral delta, Rust deletion, approval renewal, broad discovery or acceptance is claimed.

## Discovery process lifecycle

The one-time new-pin discovery exposed an RPC oracle hang: its child failed importing a removed module, but the outer request reader stayed open while Rust retained its stdin writer. No response or EOF could reach Rust. The oracle now closes its request reader and pauses stdin in `finally`, preserving the failing exit and diagnostics rather than converting the import failure to success. `oracle/rpc-oracle-lifecycle.test.mjs` holds caller stdin open and injects a child import failure; it runs through the protected `run-tests.sh oracle` gate. Product adapter/import gaps remain unresolved; this repair proves error-path termination only.

Discovery containment, exact owned process identities and logs: `/tmp/pi-rust-repin-infra/HANG.md`. Only that test oracle group was terminated; the original `--no-fail-fast` Cargo run continues without a retry. Final discovery evidence must distinguish compiled product revision and any later harness-only lifecycle repair.

## Evidence policy

All pre-repin proof logs/checklists and dated approvals in `docs/porting/traceability/`, `docs/specs/`, and prior proof reports describe their recorded original source/revisions, not new-pin acceptance. Preserve that provenance; [coverage](coverage.md) is the active per-module inventory. Unchanged rows still need oracle re-run; modified/renamed/added rows remain todo. [Inventory reconciliation](inventory.md) records 686 active rows and 213 old-only rows. The excluded 687th classification path is `packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/test.ts`, a standalone test driver excluded by the existing selection rules.

## Runtime/setup audit

All tracked `crates/`, `oracle/`, `scripts/`, `.cargo/` and `run-tests.sh` paths were searched for checkout literals, secondary locks, catalog assumptions and lock consumers. Active runtime checkout selection now reads the common lock; no old checkout literal or secondary session-name lock remains. The two asset/script fixture checkout literals now use the common lock. Supplied R-record selectors are updated exactly; deleted or D/A-replaced APIs are not guessed or adapted. No product source changed.

The protected guard mounts only the common source and its Git metadata, not an obsolete type-reference tree. Source tsconfig aliases resolve installed packages to real source despite absent dist declarations. Type-only oracle imports use lock-selected `@pi-oracle-source/*`; ambient declaration files remain included. Session preflight checks tracked `models.generated.ts`, not old generated provider JSON. Strict type reference [proof](oracle-gate.md) passes at the public-index compiler boundary, without changing any source bytes. Existing downstream API/diagnostic assertions remain unchanged and may fail.

Static literal audit found 221 distinct source-like paths, 46 absent. Two are not oracle blockers: `ai/src/autocomplete.ts` is an intentional temporary autocomplete fixture; `coding-agent/dist/cli.js` is a product returned-path assertion, not a source-oracle import. Remaining 44 candidate missing paths below map to active/removed coverage rows by path. This static inventory is not execution proof and does not predict every failing target. Full locations: `/tmp/pi-rust-repin-infra/runtime-path-audit.json`.

| Missing source/fixture path | First consumer |
|---|---|
| `agent/src/harness/agent-harness.ts` | `crates/pi-agent-core/tests/differential_agent_harness.rs:28` |
| `agent/src/harness/telemetry.ts` | `crates/pi-agent-core/tests/differential_agent_harness.rs:29` |
| `agent/src/harness/session/index.ts` | `crates/pi-agent-core/tests/differential_agent_harness.rs:30` |
| `telemetry/src/index.ts` | `crates/pi-agent-core/tests/differential_agent_harness.rs:33` |
| `agent/src/harness/events.ts` | `crates/pi-agent-core/tests/differential_harness_core/support.rs:115` |
| `agent/src/harness/reducer.ts` | `crates/pi-agent-core/tests/differential_harness_core/support.rs:120` |
| `agent/src/harness/session/jsonl/codec.ts` | `crates/pi-agent-core/tests/differential_session_codec_numbers.rs:56` |
| `agent/src/harness/session/testing/index.ts` | `crates/pi-agent-core/tests/differential_session_conformance.rs:28` |
| `agent/src/harness/utils/shell-output.ts` | `crates/pi-agent-core/tests/differential_truncate/mod.rs:38` |
| `agent/src/index.ts` | `crates/pi-agent-core/tests/differential_truncate/mod.rs:39` |
| `agent/src/harness/tools/image.ts` | `crates/pi-agent-core/tests/differential_truncate/mod.rs:43` |
| `agent/src/harness/tools/edit-diff.ts` | `crates/pi-agent-core/tests/differential_truncate/mod.rs:44` |
| `ai/src/api/cloudflare-gateway-binding.ts` | `crates/pi-ai/tests/differential_cloudflare_gateway_binding.rs:308` |
| `ai/src/api/constrained-sampling.ts` | `crates/pi-ai/tests/differential_constrained_sampling.rs:248` |
| `ai/src/utils/deferred-tools.ts` | `crates/pi-ai/tests/differential_deferred_tools.rs:67` |
| `ai/src/model-catalog.ts` | `crates/pi-ai/tests/differential_model_catalog.rs:54` |
| `ai/scripts/models-dev-reasoning-options.ts` | `crates/pi-ai/tests/differential_models_dev_reasoning_options.rs:22` |
| `ai/src/api/pi-messages.ts` | `crates/pi-ai/tests/differential_pi_messages.rs:423` |
| `ai/src/utils/text.ts` | `crates/pi-ai/tests/differential_text.rs:47` |
| `ai/scripts/model-data.ts` | `crates/pi-ai/tests/model_data_tools.rs:44` |
| `client/src/index.ts` | `crates/pi-client/tests/client/differential_client.rs:17` |
| `coding-agent/src/cli/auth-command.ts` | `crates/pi-coding-agent/tests/differential_auth_command/mod.rs:9` |
| `coding-agent/src/extensions/llama/client.ts` | `crates/pi-coding-agent/tests/differential_llama_client/mod.rs:15` |
| `coding-agent/src/extensions/llama/huggingface.ts` | `crates/pi-coding-agent/tests/differential_llama_huggingface/mod.rs:14` |
| `coding-agent/src/extensions/llama/provider.ts` | `crates/pi-coding-agent/tests/differential_llama_provider/mod.rs:24` |
| `coding-agent/src/extensions/llama/ui.ts` | `crates/pi-coding-agent/tests/differential_llama_ui/mod.rs:15` |
| `coding-agent/src/core/model-config.ts` | `crates/pi-coding-agent/tests/differential_model_config.rs:134` |
| `coding-agent/src/core/model-runtime.ts` | `crates/pi-coding-agent/tests/differential_model_runtime/mod.rs:247` |
| `evals/src/vitest-evals/artifacts.ts` | `crates/pi-evals/tests/differential_artifacts.rs:12` |
| `evals/src/vitest-evals/harness-table.ts` | `crates/pi-evals/tests/differential_harness_table.rs:15` |
| `evals/src/pi-harness.ts` | `crates/pi-evals/tests/differential_pi_harness.rs:9` |
| `evals/src/vitest-evals/summary.ts` | `crates/pi-evals/tests/differential_summary.rs:16` |
| `evals/scripts/run-evals.mjs` | `crates/pi-evals/tests/run_evals.rs:117` |
| `protocol/src/framing.ts` | `crates/pi-protocol/tests/differential_protocol/mod.rs:18` |
| `protocol/src/schemas.ts` | `crates/pi-protocol/tests/differential_protocol/mod.rs:19` |
| `protocol/src/cbor/index.ts` | `crates/pi-protocol/tests/differential_protocol/mod.rs:21` |
| `protocol/src/codec.ts` | `crates/pi-protocol/tests/differential_protocol/mod.rs:22` |
| `server/src/protocol.ts` | `crates/pi-server/tests/differential_protocol.rs:126` |
| `server/src/transports/unix/preset.ts` | `crates/pi-server/tests/differential_stream.rs:422` |
| `session-backends/sqlite-node/src/index.ts` | `crates/pi-session-sqlite/tests/cross_implementation.rs:12` |
| `agent/src/harness/tools/index.ts` | `oracle/edit-record-typecheck.mjs:15` |
| `agent/src/utils/highlight-js-lib-index.d.ts` | `oracle/edit-record-typecheck.mjs:19` |
| `coding-agent/src/core/provider-composer.ts` | `oracle/example-custom-anthropic-oracle.ts:68` |
| `server/src/testing/client.ts` | `oracle/server-conformance-client.ts:32` |

Dynamic imports also require `agent-core/src/harness/tools/{bash,edit,read,write}.ts`, absent at this pin; the old tools were deleted rather than renamed by the supplied classification. Session-conformance's old testing/index and memory modules are absent. These are explicit product/oracle reconciliation blockers, not grounds to recreate old files or weaken tests. Generated-catalog/artifact oracles still model the old split JSON/shard contract; the new tracked monolithic catalog may expose behavioral mismatches during discovery.

## Focused proof and handoff

Proof ledger: `/tmp/pi-rust-repin-infra/ledger.md`; logs carry exact tested revisions. Targeted tests cover inventory status policy/mutations, real compiler reference/negative diagnostics, lock-selected aliases, session preflight and protected runtime state. This is not broad acceptance. Readability for changed Rust/JS is deferred to main's independent verifier; no audit pass is claimed.

Main owns one discovery after integration, from the canonical checkout (Cargo itself outside the runtime namespace):

```text
CARGO_BUILD_JOBS=4 cargo test --workspace --all-targets --no-fail-fast
```

Run using argv/environment via Pyrun and retain stdout/stderr once in `/tmp/pi-rust-repin-infra/discovery.log`; configured Cargo runner protects compiled tests. Main then writes `DISCOVERY.md` with per-target pass/fail counts, primary cause and coverage-row mapping. No discovery command has been run by this slice. Missing modules/API differences above are deferred, not bypassed. No merge, push or deployment by the implementer.
