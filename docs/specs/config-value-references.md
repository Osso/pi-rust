# Coding-agent config-value references

Config-value reference classification ports the pure parsing helpers from pinned `packages/coding-agent/src/core/resolve-config-value.ts`. This bounded slice discovers environment references and command-backed values without reading the environment or executing commands.

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

## How it works

- [Config-value reference traceability](../porting/traceability/config-value-references.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/config_value.rs` — private template/reference parser and public query helpers.
- `crates/pi-coding-agent/src/core/mod.rs` — public core exports.
- `crates/pi-coding-agent/tests/config_value_references.rs` — focused parser/classification behavior.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/config_value_references.rs`

## Known gaps (current cycle)

- [ ] Add direct differential proof against the pinned TypeScript helpers.

## Out of scope

- Environment and process lookup, including empty-string truthiness behavior.
- Missing-variable and configured-status checks.
- Template interpolation and resolved values.
- Shell execution, shell selection, timeouts, and command-result caching.
- Header resolution, provider composition, authentication execution, and consumer integration.
- JavaScript lone-surrogate strings and invalid runtime input types.
