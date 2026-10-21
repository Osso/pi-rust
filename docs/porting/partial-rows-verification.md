# Partial-row verification

Date: 2026-10-20. Verifier role; read `/home/osso/AgentConfig/skills/verify/SKILL.md` before verification. No delegation.

## Result

**Seven rows can move to DONE. `ai/scripts/generate-models.ts` remains GAP.**

| `docs/porting/coverage.md` row | Verdict | Fresh targeted evidence |
|---|---|---|
| 756 — `coding-agent/examples/extensions/commands.ts` | **DONE** | `example_commands`: 1 passed |
| 760 — `coding-agent/examples/extensions/custom-header.ts` | **DONE** | `example_custom_header`: 1 passed |
| 761 — `coding-agent/examples/extensions/custom-provider-anthropic/index.ts` | **DONE** | `example_custom_anthropic`: 14 passed |
| 838 — `coding-agent/examples/sdk/01-minimal.ts` | **DONE** | `example_sdk_minimal`: 4 passed |
| 217 — `ai/scripts/check-model-data.ts` | **DONE** | Shared `model_data_tools`: 17 passed, including checker subprocess/differential cases |
| 218 — `ai/scripts/generate-image-models.ts` | **DONE** | `image_catalog_generator` 9 + `image_catalog_http` 3 + `image_catalog_prototype` 1 passed |
| 219 — `ai/scripts/generate-models.ts` | **GAP** | Its 11 existing targets: 36 passed. Additional isolated consumed-field contract probe: **0 passed, 1 failed**, exit 101 |
| 221 — `ai/scripts/model-data.ts` | **DONE** | Shared `model_data_tools`: 17 passed; same execution as checker, not counted twice |

Existing requested targets: **19 commands, 86 passed, 0 failed, 0 ignored, 0 filtered out**. Each existing target executed once. Passing existing tests do not establish completion of the generator's unfinished validation contract.

## Revision, scope, and safety

- Rust: `/syncthing/Sync/Projects/pi-rust`, clean `master`, `0dbd1cbff2c3844e0d7c646e2863bd1d6670394d`, checked before and after verification.
- TS: `/home/osso/.worktrees/pi-rust-source-532f7d12`, `532f7d12cd19235081bd4d6e425bde185652d86f`; Cargo oracle support also checks the source lock before importing actual pinned modules.
- Read each coverage row, its traceability/spec, and applicable compatibility/approval boundaries. Compared source statements with native implementations and behavioral fixtures; did not treat traceability assertions or historical GREEN as acceptance proof.
- No tracked files edited, no commits, no workspace suites/check/Clippy. No direct Bash, node, tsx, or pi execution. Commands used Pyrun `cli.*`; JS ran only as Cargo-test descendants. Mandatory `scripts/cargo-runner.sh` isolation remained enabled. Network fixtures were sandbox-local loopback, not live providers. No live Pi configuration or baseline catalog refresh.
- `DONE` here means the requested row-level behavior/deviation acceptance, not a claim that unrequested workspace gates ran. Source branches outside approved native/typed representability boundaries are not silently treated as runtime parity.

Paths below are relative to the Rust repository unless prefixed `TS:`. `TS:` means the pinned worktree path above, plus `packages/`.

## 1. commands.ts — DONE

Read: `docs/porting/traceability/example-commands.md`, `docs/specs/example-commands.md`, compatibility typed-input boundary.

| Pinned behavior | Native implementation | Passing behavioral proof |
|---|---|---|
| TS `coding-agent/examples/extensions/commands.ts:15-22`: exact command metadata, ordered prefix completions, null when none | `crates/pi-coding-agent/examples/extensions/commands.rs:11-29` | `crates/pi-coding-agent/tests/example_commands.rs:194-227`: complete metadata/completion/transcript equality with actual TS import |
| TS `:23-53`: fresh command snapshot, JS trim, source filtering, exact empty notices, fixed group order and description formatting | Native `:32-60`, `:96-107` | Fixture `tests/example_commands.rs:75-125`: empty/changing lists, all source groups, case/plural/arbitrary filters, BOM/NBSP/U+2028 versus non-trim U+0085/U+200B, missing/empty/nonempty descriptions |
| TS `:56-69`: selection/cancellation/header guards; split at ` - `, remove one UTF-16 unit; first original duplicate; conditional path confirmation/notice | Native `:61-92` | Same exact full-transcript differential: cancellation, all headers, unknown selection, duplicate outside filtered group, separator/slash names, empty path, both confirmation outcomes, scalar Unicode |

Native source embedding/explicit factory selection is the approved extension adaptation, not an autoloading gap. Invalid JS runtime types and unpaired-surrogate inputs excluded by this example's documented native String boundary. Source oddities (singular `extension`, whole-original-list duplicate resolution, separator truncation) are retained, not silently fixed. No remaining module branch gap found within that boundary.

## 2. custom-header.ts — DONE

Read: `docs/porting/traceability/example-custom-header.md`, `docs/specs/example-custom-header.md`.

| Pinned behavior | Native implementation | Passing behavioral proof |
|---|---|---|
| TS `coding-agent/examples/extensions/custom-header.ts:14-45`: exact eight mascot lines, spaces/glyphs and theme roles | `crates/pi-coding-agent/examples/extensions/custom_header.rs:30-49` | `tests/example_custom_header.rs`, `pinned_custom_header_composition_matches_all_modes_and_colors`: entire vectors and ANSI bytes, not ANSI-stripped text |
| TS `:47-63`: session-start installs only in TUI; factory theme; width ignored; package VERSION subtitle; no-op invalidation | Native `:15-27`, `:51-64` | Four modes × two RGB themes; widths 0, 1, 16, 80, 200; post-invalidation rerender; no UI effect at load. Fixture package VERSION is controlled consistently through public APIs |
| TS `:65-73`: `/builtin-header`, exact description, clear before info notification, args ignored | Native `:66-80` | Same differential, all modes, complete UI-call sequence |

`invalidate()` is intentionally empty because TS is stateless, not a stub. Explicit embedding rather than TypeScript loading/default autoload is approved. No remaining branch gap found.

## 3. custom-provider-anthropic/index.ts — DONE

Read: `docs/porting/traceability/example-custom-anthropic.md`, `docs/specs/example-custom-anthropic.md`, compatibility snapshot/typed boundaries.

| Pinned behavior | Native implementation | Passing behavioral proof |
|---|---|---|
| TS `coding-agent/examples/extensions/custom-provider-anthropic/index.ts:60-153`: PKCE, URL/prompt, code/state splitting, token exchange/refresh, five-minute expiry adjustment, credential access | `crates/pi-coding-agent/examples/extensions/custom_provider_anthropic.rs:49-152` | `tests/example_custom_anthropic.rs:245` OAuth differential: deterministic random/clock, split variants and callback/status/transport failures |
| TS `:163-336`, `:362-446`: OAuth detection/tool names, literal surrogate sanitizer, message/image/thinking/tool-result conversion, cache, tools, budgets and request | Example `request.rs:19-200`, `stream.rs:74-133` | `support/custom_anthropic_cases.rs`: complete request matrix, including every reasoning level, typed nonfinite budgets and supplementary-scalar sanitizer behavior |
| TS `:449-567`: start/content/delta/stop, usage replacement/cost, stop reasons, abort/errors, `index`/`partialJson` lifetime | Example `stream.rs:14-71`, `:134-248` | Cases/lifecycle targets included in the 14-test run: valid start boundary, repeated/unknown index, mismatched delta, signature, incomplete JSON, unclosed blocks, error cleanup; object-only rejection explicitly tested |
| TS `:577-612`: exact provider/catalog/OAuth registration and API-key binding | Main example `:28-46`, `:157-195` | Exact catalog factory plus real composed registration/env/credential availability differential |
| Actual installed SDK HTTP and SSE boundary | Existing native SDK/request/reader reached by example stream | Both `wire::installed_sdk_wire_auth_body_and_events` and `wire::installed_sdk_wire_status_error_and_terminated_body` passed: API key/OAuth headers, body, all events/results, status 400 and mid-body disconnect |

Approvals are narrow: Rust-native opt-in extension, object-only tool arguments with explicit failure preserving scratch, immutable push snapshots, native runtime/version identity, and precisely four Node HTTP defaults (`connection`, `accept-encoding`, `accept-language`, `sec-fetch-mode`). Wire tests assert those differences before comparing remaining headers; they do not discard arbitrary headers or event fields. Native causes/JS-frame handling follows recorded transport policy. Installed root SDK is 0.91.1, not the example package's declared 0.52.0; report does not claim a 0.52.0 install/test. The controlled unchanged-source producer differential and actual available-SDK wire proof are separate. No remaining native-example branch gap found within recorded boundaries.

## 4. sdk/01-minimal.ts — DONE

Read: `docs/porting/traceability/example-sdk-minimal.md`, `docs/specs/example-sdk-minimal.md`.

| Pinned behavior | Native implementation | Passing behavioral proof |
|---|---|---|
| TS `coding-agent/examples/sdk/01-minimal.ts:10`: default factory call, before try/finally | `crates/pi-coding-agent/examples/sdk/01_minimal.rs:13`, `:65-70` | Whole-source controlled factory oracle asserts zero arguments; native main compiled and supplies defaults |
| TS `:12-19`: subscribe before sole literal prompt, write only message_update/text_delta | Native `:17-39` | `tests/example_sdk_minimal.rs:161-195`: exact subscription/prompt/write ordering and non-text filtering; actual SDK/Faux stream success fixture |
| TS `:20-26`: print every message in order plus final blank line; dispose success/failure | Native `:40-52` | `tests/example_sdk_minimal.rs:198-256`: every printed message equals reopened persisted state, exact stream prefix/order/newlines; `:292-322`: prompt failure emits no messages and releases resources |
| Failure name/message and failure status | Native `ExampleError`, `:55-70` | `tests/example_sdk_minimal.rs:259-289`: isolated termination subprocess, exit 0/1 and exact `Error:` stderr with quotes/newlines |

Explicit 2026-10-20 approval: “JSON + omit JS frames (Recommended)”. Tests preserve every source log argument/message value and normalize only approved object display syntax; no literal Node-inspect-byte claim. Default/live discovery/provider execution is intentionally not invoked; it is neither needed nor safe for this isolated composition proof. No remaining example branch gap found.

## 5. check-model-data.ts — DONE

Read: `docs/porting/traceability/check-model-data.md`, shared model-data trace/spec.

TS `ai/scripts/check-model-data.ts:7-16` resolves the package from source location, validates, logs exact success or error + hydration guidance, and sets exitCode 1 on failure. Native `crates/pi-ai/src/bin/check-model-data.rs:7-21` uses the compile-time crate root, compiled expected model identities/APIs, and native `data/providers`, then retains those exact streams/statuses.

This layout is explicitly authorized by “Check Rust catalog (Recommended)” on 2026-10-20. It does **not** authorize CWD inference; native correctly uses `CARGO_MANIFEST_DIR`.

`crates/pi-ai/tests/model_data_tools.rs:347-370` compares whole stdout/stderr/status to the real pinned checker over a fresh native catalog fixture, valid/schema-error cases, and two unrelated CWDs. `:556-651` overlays only temporary fixture data read-only over the exact compiled catalog path in a network-unshared child; separate source-root proof removes fixture TS source files. Shared target passed 17/17.

The checked-in stale manifest (40 provider files, 39 hashes, `kimi-coding.json` absent from manifest) remains an existing data-state discrepancy, not a checker implementation defect. No live catalog mutation or false claim that baseline data validates.

## 6. generate-image-models.ts — DONE

Read: `docs/porting/traceability/image-catalog-generator.md`, `docs/specs/image-catalog-generator.md` and its exact approvals.

| Pinned behavior | Native implementation | Passing behavioral proof |
|---|---|---|
| TS `ai/scripts/generate-image-models.ts:35-92`: raw list shape, modalities/dedup, image requirement, first usable ID, name and prices/defaults | `crates/pi-ai/src/image_catalog_generator.rs:83-202` | Parser 18 payloads × both modes; required-name and raw-ID/name matrices; missing/nullish containers/defaults, skip/dedup unread fields, 128 valid shuffled IDs |
| TS `:13-18`, `:94-108`, `:150-165`: flags → GET/status/JSON/parser → logs → write/catch/status | Native `:206-213`, `:288-375`; maintenance bin | 23-case workflow corpus; exact real-script streams/events/exit; unknown args before fetch, non-strict empty write versus strict stop, filesystem failures |
| TS `:110-148`: locale sorting and generated model values/key order | Native `:216-286` | Real generated TS artifact imported by oracle, every model/key compared; numeric keys/surrogates/nonfinite serialization; prototype test retains classified `__proto__` disappearance |
| Actual request/body/file boundaries | Native shared Fetch/read-body/Node FS | All three `image_catalog_http` tests: chunked Unicode/surrogate transport to temporary file, refused connection and premature body in both strict modes |

Approvals: native JSON layout; JS-stack omission only; wrong **consumed** fields reject with native TypeErrors at original read/skip/dedup boundaries; native HTTP causes only. Missing names lack a source default and reject only on usable nonduplicates. Present prices require strings, but string NaN/Infinity/negative-zero behavior is retained. No blanket malformed-input exclusion. The current tests assert the approved alternatives instead of silently omitting those fixtures. All 13 own tests passed. No remaining branch gap found within those approvals.

## 7. generate-models.ts — GAP

Read: `docs/porting/traceability/model-catalog-generator.md` in full and `docs/specs/model-catalog-generator.md`, including the partial-validation wording and exact recorded policy.

### Existing capabilities freshly proven

The 36 passing tests cover cost/provider transformations, HTTP strict/default catches, four-fetcher status/transport/JSON errors, NVIDIA alias/overwrite order, Qwen exact sets, options/deleted CWD, UTF-16 raw values, deferred reasoning state, 675 metadata cases plus 144 URL/compat cases, all-provider models.dev fixtures, complete catalog merges/overrides/clones/dedup/sort, native/private/public artifact values/manifests/include compilation, eight transaction failures, data-only guards, public-output failures, native transport identity and original automatic CLI catch. These pass now; several pending notes in coverage row 219 are stale. They do not close the following consumed-field hole.

### Exact uncovered behavior and classification

**Defect in the approved native replacement validation contract:** a present, consumed `ModelsDevModel.reasoning` with the wrong type is converted to `false` instead of rejected. It is not an approved permanent exemption.

- TS declaration: `TS:ai/scripts/generate-models.ts:83-89`, specifically `reasoning?: boolean` at **line 88**.
- Source consumption: `TS:ai/scripts/generate-models.ts:1211-1215`, specifically `const reasoning = model.reasoning === true` at **line 1214**, after deprecated-record filtering. The pinned runtime consequently computes false for a string; that is the source coercion/type accident the native validation approval is intended to replace, not evidence of a TS-runtime mismatch on this individual input.
- Rust consumption: `crates/pi-ai/src/model_catalog_generator.rs:143-155`, specifically **line 154**, computes `JsValue::Bool(field(model, "reasoning") == &JsValue::Bool(true))` before any boolean check.
- `crates/pi-ai/src/model_catalog_generator/receiver_types.rs:20-22` checks only that the model is an object. It does not validate the present reasoning field.
- `crates/pi-ai/src/model_catalog_generator/emission_types.rs:61` checks the **already converted output boolean**. It cannot detect the malformed original input.
- The input below is neither a deprecated/skipped record nor an unknown string-valued control. `reasoning_options[].type` may legitimately contain unknown strings; the distinct consumed field `reasoning` is declared boolean.

Concrete fixture:

```json
{"models":{"ordinary":{"name":"Ordinary","reasoning":"yes"}}}
```

A throwaway out-of-tree Cargo probe calls the unchanged public `process_baseten_models` implementation and requires the approved native `TypeError`. It fails at `/tmp/pi-rust-verify-partial/probe/tests/consumed_fields.rs:8:22`:

```text
test consumed_declared_reasoning_boolean_rejects_wrong_present_type ... FAILED
approved consumed-field policy requires rejecting present wrong-typed reasoning; accepted [{"id":"ordinary","name":"Ordinary","api":"openai-completions","provider":"baseten","baseUrl":"https://inference.baseten.co/v1","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"compat":{"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":false,"supportsUsageInStreaming":true,"maxTokensField":"max_tokens","supportsStrictMode":true,"supportsLongCacheRetention":false},"contextWindow":4096,"maxTokens":4096}]
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

This is a behavioral input/output test, not an assertion about implementation shape. Fresh native observation is reproduced; the specific TS coercion result is established directly from the pinned `=== true` source, not claimed as an extra executed TS fixture. Probe source and full output retained alongside this report. No production fix attempted.

### Is “Approved consumed-field validation remains partial” permanent approval?

**No: unfinished work, not a permanent approved deviation.**

`docs/specs/model-catalog-generator.md:8` explicitly says “Remaining response/metadata branches stay partial”. The recorded 2026-10-20 decision is **“Reject wrong field types (Recommended)”**, not “skip remaining validation”. `docs/porting/traceability/model-catalog-generator.md:55` makes the boundary explicit: **“present consumed fields use declared types or actual operations/Model contract for `any`”** and says existing ID/emission/receiver guards **“do not imply every malformed response branch has been audited.”**

The later trace text saying no new audit was implied does not record an Alessio approval to waive that contract or turn partial coverage into complete coverage. The concrete failing boolean probe shows the distinction matters. Keep row 219 partial until this consumed-field behavior and the remaining acknowledged validation coverage are implemented/proven, or Alessio explicitly approves a narrower permanent boundary. This report does not assert that every other malformed-response branch has been exhaustively enumerated.

## 8. model-data.ts — DONE

Read: `docs/porting/traceability/model-data-tools.md`, `docs/specs/model-data-tools.md`; shared Date/UTF-16 and typed-input boundaries are linked by those docs.

| Pinned behavior | Native implementation | Passing behavioral proof |
|---|---|---|
| TS `ai/scripts/model-data.ts:5-46`, `:117-137`: constants, exact dedup/sort/set differences, nested structure hash and schema-3 manifest | `crates/pi-ai/src/model_data_tools.rs:14-16`, `:41-65`, `:196-240`, `:373-397` | Exact-ID/manifest tests; complete UTF-16 API responses, numeric-key enumeration and Node UTF-8 hashing boundaries |
| TS `:48-115`: JSON-object read errors, exact generated import grammar, provider shard agreement, duplicate/empty/API structure | Native `:268-371` | Aggregator/shard/structure and filesystem fixtures; all JS line terminators; raw surrogate keys/API identity |
| TS `:139-276`: metadata/schema/timestamp/hash/file sets, models/grouping, ordered accumulated errors and first-30 suffix | Native `:405-613` | Model validation matrix, malformed manifests/files, duplicate groups, missing/extra IDs, numeric diagnostic order, >30 errors; exact complete source responses, not message-substring-only acceptance |
| TS `:278-281`: exported full generated-layout validation | Native `:636-639` | Real temporary TS-layout fixtures, success and failures, independent of the separately approved native checker layout |

Raw JSON is dynamically validated and retains lossless UTF-16 keys/values/diagnostics; it is not excluded by the typed API approval. Timestamp validation passes original UTF-16 units to the shared V8 parser, including a lone surrogate inside an ignored date comment. Invalid argument *types* at typed structure/manifest/path/allowlist APIs remain the documented approval. Shared target passed 17/17. No remaining module branch gap found within that boundary; no assertion of a live baseline refresh or new exhaustive all-Date.parse-grammar proof.

## Artifact checks

- **[EXIST] PASS:** all 18 native implementation/module/bin files inspected exist; exact byte/line counts in `artifact-audit.txt`.
- **[SUBSTANTIVE] PASS:** real command/session handlers, OAuth/request/producer, SDK composition, parser/workflow, hashes/validators, generator transforms/transactions. No `todo!`/`unimplemented!` placeholders. Header invalidation is intentionally source-empty.
- **[WIRED] PASS:** example sources compiled via `#[path]` from tests (`example_commands.rs:3-4`, `example_custom_header.rs:2-3`, `example_custom_anthropic.rs:10-11`, `example_sdk_minimal.rs:2-3`). Minimal executable declared in `crates/pi-coding-agent/Cargo.toml:101-103`. Maintenance modules exported at `crates/pi-ai/src/lib.rs:11,19-20` and imported by their bins (`check-model-data.rs:5`, `generate-models.rs:2`, `generate-image-models.rs:8`). Opt-in extension embedding is intentional, not an orphaned default-runtime feature.
- **[ANTI-PATTERN] PASS:** zero TODO/FIXME/HACK/XXX/todo!/unimplemented! markers across the 18 inspected implementation files. No empty error-swallowing Rust catch blocks. This scan does not override the behavioral generator failure.
- **OVERALL:** requested row-level verification is complete; seven DONE verdicts, one GAP. Not an all-eight completion claim.

## Commands/results ledger

All commands below ran against the exact Rust revision stated above. No subsequent production changes invalidate their proof. stdout and stderr files contain complete captured streams. `ledger.json` adds durations and exact scope/retry reasons.

| Command | Result | Raw streams (relative to report directory) |
|---|---|---|
| `cargo test -p pi-coding-agent --test example_commands` | exit 0; 1 passed | `example_commands.stdout`, `.stderr` |
| `cargo test -p pi-coding-agent --test example_custom_header` | exit 0; 1 passed | `example_custom_header.stdout`, `.stderr` |
| `cargo test -p pi-coding-agent --test example_custom_anthropic` | exit 0; 14 passed | `example_custom_anthropic.stdout`, `.stderr` |
| `cargo test -p pi-coding-agent --test example_sdk_minimal` | exit 0; 4 passed | `example_sdk_minimal.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_data_tools` | exit 0; 17 passed | `model_data_tools.stdout`, `.stderr` |
| `cargo test -p pi-ai --test image_catalog_generator` | exit 0; 9 passed | `image_catalog_generator.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator` | exit 0; 1 passed | `model_catalog_generator.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_http` | exit 0; 3 passed | `model_catalog_generator_http.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_fetch` | exit 0; 4 passed | `model_catalog_generator_fetch.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_native_http` | exit 0; 2 passed | `model_catalog_generator_native_http.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_options` | exit 0; 2 passed | `model_catalog_generator_options.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_raw` | exit 0; 1 passed | `model_catalog_generator_raw.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_receivers` | exit 0; 7 passed | `model_catalog_generator_receivers.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_metadata` | exit 0; 3 passed | `model_catalog_generator_metadata.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_models_dev` | exit 0; 2 passed | `model_catalog_generator_models_dev.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_workflow` | exit 0; 3 passed | `model_catalog_generator_workflow.stdout`, `.stderr` |
| `cargo test -p pi-ai --test model_catalog_generator_artifacts` | exit 0; 8 passed | `model_catalog_generator_artifacts.stdout`, `.stderr` |
| `cargo test -p pi-ai --test image_catalog_http` | exit 0; 3 passed | `image_catalog_http.stdout`, `.stderr` |
| `cargo test -p pi-ai --test image_catalog_prototype` | exit 0; 1 passed | `image_catalog_prototype.stdout`, `.stderr` |

Additional defect probe command:

```text
cargo test --offline --manifest-path /tmp/pi-rust-verify-partial/probe/Cargo.toml --target-dir /syncthing/Sync/Projects/pi-rust/target --test consumed_fields
```

Initial execution: exit 1 **before test execution**; private `/tmp` hid the temporary package CWD from bwrap. Logs: `probe.stdout`, `probe.stderr`. Not behavioral evidence, not a production defect. No unsandboxed fallback.

Retry changed only the test environment to `PI_TEST_SANDBOX_DIR=/tmp/pi-rust-verify-partial`, explicitly binding that private proof directory through the existing sandbox. Same probe command then ran the test: exit **101**, **0 passed / 1 failed**, zero ignored/filtered. Logs: `probe-isolated.stdout`, `probe-isolated.stderr`. It reproduced the validation gap above. Existing requested targets were not rerun.

Administrative observations: initial `git status`/`rev-parse` without explicit Pyrun CWD failed with exit 128 because the runtime starts outside the repository; corrected to `.cwd("/syncthing/Sync/Projects/pi-rust")`, both exit 0. A later evaluation relying on prior Python globals raised NameError before executing commands; subsequent evaluations defined paths locally. Neither produced test evidence nor changed files. Final explicit-CWD `git status --short --branch` returned `## master` and final HEAD remained `0dbd1cbff2c3844e0d7c646e2863bd1d6670394d`; TS HEAD matched its pin. Report/probe/logs are outside the tracked checkout.
