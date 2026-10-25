# Coding-agent config-value references

Config-value behavior ports selected helpers from pinned `packages/coding-agent/src/core/resolve-config-value.ts`. Current coverage discovers references, classifies command-backed values, and reports environment-backed configured status without resolving templates or executing commands.

## What it must do

### Environment references

- [x] Return a borrowed name only when the complete non-command value is exactly one `$NAME` or `${NAME}` reference.
- [x] Recognize case-sensitive ASCII names beginning with a letter or underscore and continuing with letters, digits, or underscores.
- [x] Let unbraced references consume the longest valid prefix while requiring a complete valid name inside braces.
- [x] Return unique names in first-seen order across adjacent, repeated, bare, and braced references.

### Literal and malformed syntax

- [x] Interpret `$$` as literal `$` and `$!` as literal `!`.
- [x] Preserve unrecognized and lone dollars as literal text.
- [x] Preserve invalid braced segments through the first closing brace as literal text.
- [x] Rescan after an unterminated `${`, allowing later valid dollar references to be discovered.
- [x] Treat Unicode, supplementary scalars, NUL, and all non-name characters as literal data.

### Commands

- [x] Classify every value beginning with exact `!` as a command, including `!` alone.
- [x] Give command classification precedence over environment-reference parsing.
- [x] Keep escaped or non-leading exclamation marks as template literals.

### Environment status

- [x] Prefer nonempty explicit string-map values over the process environment.
- [x] Let empty explicit values fall through to process-environment lookup.
- [x] Treat empty or absent process values as missing and every nonempty process value as configured.
- [x] Preserve unique missing-name order inherited from reference parsing.
- [x] Treat literals and commands as configured because they require no environment names; do not execute commands.
- [x] Handle Unicode and non-Unicode process values through deterministic child-process proof without mutating global test environment state.

## How it works

- [Config-value reference traceability](../porting/traceability/config-value-references.md)

## Implementation inventory

- `crates/pi-coding-agent-data/src/core/config_value.rs` — private template parser plus public reference and environment-status helpers.
- `crates/pi-coding-agent-core/src/core/mod.rs` — public core exports.
- `crates/pi-coding-agent/tests/config_value_references.rs` — focused parser/classification behavior.
- `crates/pi-coding-agent/tests/config_value_status.rs` — focused explicit-map and deterministic process-environment behavior.
- `crates/pi-coding-agent/tests/support/config_value_status_probe.rs` — shared self-spawned child-process probe.
- `oracle/config-value-reference-oracle.ts` — persistent adapter invoking the real pinned helpers.
- `crates/pi-coding-agent/tests/differential_config_value_references.rs` — direct comparison entry point.
- `crates/pi-coding-agent/tests/differential_config_value_references/fixtures.rs` — exact reference parser/classification fixtures.
- `crates/pi-coding-agent/tests/differential_config_value_references/status.rs` — explicit/process environment status fixtures, including Unix non-Unicode bytes.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/config_value_references.rs`
- `crates/pi-coding-agent/tests/config_value_status.rs`
- `crates/pi-coding-agent/tests/differential_config_value_references.rs`
- `crates/pi-coding-agent/tests/differential_config_value_references/fixtures.rs`
- `crates/pi-coding-agent/tests/differential_config_value_references/status.rs`

## Known gaps (current cycle)

None for the bounded reference-classification and environment-status slices.

## Out of scope

- Returning or interpolating environment values; selected status helpers observe only nonempty presence.
- Template interpolation and resolved values.
- Shell execution, shell selection, timeouts, and command-result caching.
- Header resolution, provider composition, authentication execution, and consumer integration.
- JavaScript lone-surrogate strings and invalid runtime input types.
