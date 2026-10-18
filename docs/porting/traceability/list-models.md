# Coding-agent list models traceability

Pinned source: `packages/coding-agent/src/cli/list-models.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/list_models.rs`.

Differential: `tests/differential_list_models.rs` drives the real TS module with a duck-typed runtime and compares warning + stdout byte-for-byte across 9 fixtures (`28300..=28308`). The only substitution replaces the install directory inside the no-models guidance text.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `list-models.ts:15-25` | `formatTokenCount`: M/K suffixes, integers without decimals, otherwise one decimal | Differential fixtures `28302` (1M, 200K, 1.2M, 1.3M, 1000.0K, 1.1M, 2.0M, 999, 0) |
| `list-models.ts:30-34` | Load error prints a yellow stderr warning before listing | Fixture `28301` |
| `list-models.ts:36-41` | Empty catalog prints `formatNoModelsAvailableMessage()` | Fixtures `28300..=28301` |
| `list-models.ts:44-48` | Search uses `fuzzyFilter` over `"<provider> <id>"` | Fixtures `28304..=28307` |
| `list-models.ts:50-53` | No matches prints `No models matching "<pattern>"` | Fixture `28306` |
| `list-models.ts:56-61` | Sort by provider then id with `localeCompare` | Fixture `28303` |
| `list-models.ts:63-97` | Column widths, two-space separators, six headers, `yes`/`no` flags | All fixtures |

## Semantics

- `ListModelsRuntime` mirrors the source's structural runtime dependency (`getError`/`getAvailable`) so the differential can inject fixtures; `ModelRuntime` implements it.
- `format_token_count` uses `pi_js_compat::number::to_fixed` (new shared helper) because ECMAScript `toFixed(1)` rounds the exact double value; Rust `{:.1}` and `(value*10).round()` both diverge (for example `1.15` and `9/8`).
- Padding uses UTF-16 lengths, matching `String.prototype.padEnd`.

## Gaps

- The `AbortSignal` path is forwarded but not exercised by fixtures (cancellation is tested by the model-runtime slice).

## Discrepancies

None found.
