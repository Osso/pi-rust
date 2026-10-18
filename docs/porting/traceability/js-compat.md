# `pi-js-compat`

Shared JavaScript-runtime semantics for the Rust port. The crate is a
refactoring source of truth: it replaces the duplicated helpers that grew inside
`pi-ai`, `pi-agent-core`, `pi-tui`, and `pi-coding-agent`. There is no
TypeScript module behind this crate.

## `String.prototype.localeCompare` (`src/collator.rs`)

| Source behaviour | Rust item | Proof |
|---|---|---|
| `a.localeCompare(b)` in the host locale | `locale_compare`; host locale from `LC_ALL`/`LC_COLLATE`/`LANG` (first non-empty), language part only, `_` → `-`, fallback `und`; ICU4X `Collator` at `Strength::Tertiary` | `tests/differential_locale_compare.rs::matches_node_default_locale_compare` (Node `a.localeCompare(b)` under `LANG=en_US.UTF-8`), `host_locale_compare_uses_the_resolved_environment_locale` |
| `new Intl.Collator(locale).compare(a, b)` | `compare_in_locale(a, b, locale)` | `matches_node_intl_collator_for_explicit_locales` over `en-US`, `und`, `sv`, `de`, `tr`, `ja`, `zh` |
| Invalid locale identifier | Parses to `und`; ICU4X construction failure falls back to the root collator | Construction path shared by both tests |
| Host locale resolution | `host_locale` | `host_locale_compare_uses_the_resolved_environment_locale` |

Differential corpus: 62 pairs covering empty strings, case, accents (precomposed
and decomposed), `ß`/`ss`, Nordic and Turkish letters, digits and numeric-looking
names, punctuation (`-`, `_`, `.`, ` `, `/`, `+`, `@`), CJK, Cyrillic, Greek,
Arabic, Hebrew, and emoji. Node returns an arbitrary-magnitude number, so the
oracle normalizes both sides to the sign of the comparison.

Deviations: none observed for the corpus. ICU4X and Node's ICU4C use the same
CLDR collation data for the tested locales.

## Migrated callers

| Old path | New path |
|---|---|
| `crates/pi-coding-agent/src/core/detached_job_retention.rs` (`compare_paths`) | `pi_js_compat::locale_compare` |
| `crates/pi-agent-core/src/harness/{skills,prompt_templates}.rs` (ASCII approximation) | `pi_js_compat::locale_compare` |
| `crates/pi-tui/src/autocomplete.rs` (ASCII approximation) | `pi_js_compat::locale_compare` |
