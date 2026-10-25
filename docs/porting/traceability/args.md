# Coding-agent CLI args traceability

Frozen source: `packages/coding-agent/src/cli/args.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (550 lines).
Rust: `crates/pi-coding-agent-data/src/cli/args.rs`.

Whole-module audit found no remaining production mismatch. The former oracle whitelist hid `noSupervisor` and `refreshModels`; it now snapshots every parsed field and only converts the JS Map to ordered entries. Native snapshots include both fields.

| Source symbol/branch | Native representation | Proof |
|---|---|---|
| `Args`, `Mode`, diagnostics and optional fields | `Args`, `Mode`, `Diagnostic`; absent fields remain absent | Complete parsed-result differential |
| `isValidThinkingLevel`: eight literals | `is_valid_thinking_level`, `ThinkingLevel` | Source valid/invalid/case/whitespace matrix |
| `parseArgs`: aliases, single values, missing values and consumption | `parse_args` | Retained 45 source cases plus every built-in flag with seven ordinary next-token states |
| Repeated append/resource flags; latest single value; invalid mode/thinking preserves earlier accepted value | Same parser | Source repeated/combined cases |
| Models preserve empty trimmed items; tools/exclusions discard them | `split_comma_list`, `split_non_empty_list` | Existing native and source cases |
| Approval preset/policy validation and bypass mapping | Native approval helpers | Exact source diagnostic/value cases |
| Print consumes ordinary/triple-dash prompts, not file/flag args | Native print branch | Source/native prompt-consumption cases |
| Terminal export values, width validation, intent including equals syntax | Parser + `is_terminal_export_invocation` | Retained 16 source export cases and ordinary flag/value matrix |
| Terminal export allowed fields and incompatible data/flags | `collect_terminal_export_diagnostics` | Exact source diagnostics; optional false/empty values retain field presence |
| Standalone requires no-session; conflicts in fixed source order | `collect_standalone_worker_diagnostics` | Source full conflict list in both argv orders |
| List models/search, refresh models, tools/extensions/login | Same parser | Complete result includes refreshModels; empty/flag/file/missing next tokens |
| Project trust, offline, verbosity, files/messages | Same parser | Native + frozen complete snapshots |
| Unknown long flags: equals/value/true; overwrite preserves Map order; unknown short diagnostics | `IndexMap`, `UnknownFlagValue` | Source duplicate/order and missing/flag/file/value cases |
| `printHelp`: full text, default stdout Chalk, extension rows/default description/UTF-16 padding | `print_help`, `extension_flags_text` | Exact frozen full-help output with/without extension flags; raw extension path retained until stdout encoding |

## Semantics and gaps

- Accepted `exportColumns` is a positive safe integer; native `u64` represents that validated result exactly.
- `--tui-mode` is source-absent and stays unknown-flag data. Its old parser branch/help entry are removed; no compatibility path remains.
- Adversarial/exotic inputs, lone surrogates beyond existing carriers, numeric extremes and OS/runtime oddities follow the repository parity-bar gap. No new carrier is introduced for them.

## Proof

At `95c462006`: `cargo test -j3 -p pi-coding-agent --test suite_core_1 args:: -- --nocapture` passes all 27 selected tests. Frozen source remains unmodified. Log: `/tmp/settings-models-args-green.log`. Strict oracle typecheck and targeted warning-mode clippy pass, with no warnings in changed test code. Manual changed Rust/TS readability passes.
