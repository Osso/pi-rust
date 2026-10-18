# latex.ts traceability

Source: `packages/tui/src/latex.ts` (1373 lines). Rust: `crates/pi-tui/src/latex.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface RenderLatexOptions { display?: boolean }` | `struct RenderLatexOptions { display: bool }` (+ `Default`) | `tests/latex.rs` display cases |
| `renderLatex(source, options)` | `render_latex(&str, RenderLatexOptions) -> Option<String>` | `tests/latex.rs` (22 tests, all TS cases); `tests/differential_latex.rs` |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `render()` rejects unsupported or trailing input | `LatexParser::render` (`!supported \|\| position != len`) | `returns_undefined_for_unsupported_commands`, `returns_undefined_for_malformed_groups_and_environments` |
| `parseSequence` end character / stray `}` / nested `{` | `parse_sequence` | all group cases, differential `{`, `}` |
| command negative spacing trims trailing output | `parse_sequence` `NEGATIVE_SPACE` branch | differential `\!` generated atoms |
| `^`/`_` scripts preserve named-operator markers | `parse_sequence` script branch | `normalizes_relation_multiplication_and_named_operator_spacing`, differential |
| whitespace collapse | `parse_whitespace` | differential newline/tab cases |
| `=`/`<`/`>` spacing | relation branch | `normalizes_relation...`, differential |
| `&`, `~`, `.` after matrix | `parse_sequence` branches | `composes_matrices...` (trailing `.`), aligned/cases cases |
| command dispatch order (`\\`, spacing, ignored, escaped chars, `\|`, `not`, limits, symbols, named operators, size, delimiters, fractions, roots, boxes, binomials, accents, `mathbb`, `operatorname`, modular, overlays, wrappers, environments) | `dispatch_command` in the same order | TS case suite; generated differential corpus over 47 commands |
| `\not` negated symbol table / combining slash fallback | `parse_negation` | `extended_symbols_and_negated_relations`, differential `\not\approx`, `\not x`, `\not` |
| `\limits`/`\nolimits` modifier | `match_modifier` | `stacks_operator_limits_in_display_mode`, differential |
| duplicate lower/upper scripts mark unsupported | `parse_operator` (`is_some` checks) | generated differential `x_1^2` |
| display limit layout nodes | `LayoutNode::Operator` + `render_node_layout` | display limit cases |
| inline lower style bracket vs script | `LowerStyle` | `\lim`, `\sum` cases |
| `\frac`/`\dfrac`/`\tfrac` stacking rules | `parse_fraction_command` | `stacks_fractions_in_display_mode`, `keeps_fractions_linear...`, differential |
| `\sqrt` optional degree (2/3/4/other) | `parse_root` | `indexed_roots_accents_and_wrappers`, differential |
| accents single character vs wrapper | `dispatch_command` accent branch | `accents_and_binomial_notation` |
| `\mathbb` blackboard map with character fallback | `dispatch_command` mathbb branch | `jacobian...` (`\mathbb{C}`, `\mathbb{P}`), differential |
| `\operatorname` star + bracket limits | `parse_operatorname` | named/overlaid operator cases |
| `\mod`, `\bmod`, `\pmod`, `\pod` | dispatch branches | `named_modular...` |
| `\overset`, `\stackrel`, `\underset` | dispatch branches | `named_modular...` |
| plain wrappers (`text*`, `mbox`, `mathcal`, `underbrace`, ...) | `PLAIN_WRAPPERS` branch | `\text`, `\underbrace`, differential generated |
| environments: equation/displaymath, aligned family, cases, matrix family | `parse_environment`, `parse_aligned_environment`, `parse_cases_environment`, `render_matrix` | environment cases, differential over 14 environments × display |
| `alignedat` column pairing and `{n}` prefix removal | `parse_aligned_environment` + `ALIGNED_AT_PREFIX` | `additional_display_environments` |
| `cases` natural-language condition prefix and row delimiters | `parse_cases_environment` + `has_case_prefix` | natural case cases, differential |
| matrix column padding with protected spaces | `render_matrix` | `natural_case...`, matrix session, differential |
| matrix delimiters / unknown delimiter unsupported | `matrix_delimiters` | matrix session, differential |
| single-row matrix returns inline text | `render_matrix` (`lines.len() <= 1`) | differential `\begin{cases}\end{cases}` |
| nested rendering inherits `display && stackFractions` | `render_nested` | nested display fraction cases |
| layout: fraction stacking, operator stacking, matrix baselines, padding, marker scanning | `render_layout`, `render_node_layout`, `join_layouts`, `pad_layout_line` | all display cases, differential display corpus |
| final indentation removal and `trimEnd` | `render_latex` | matrix composition display cases |
| named-operator spacing patterns (left/right) | `normalize_output` | `\sin`, `\det`, `\deg`, differential |
| protected-space replacement | `render_latex` both paths | all matrix cases |

## Behavioral notes

- The renderer is a byte-for-byte port: private-use markers (`U+F0000`–`U+F0005`), protected spaces (`U+F0002`), and the exact spacing/indentation rules are preserved. Differential compares full output strings.
- JavaScript string indexing is by UTF-16 code unit; Rust indexes by UTF-8 byte. The parser only uses indices to locate ASCII delimiters (`\`, `{`, `}`, `[`, `]`, `$`, `%`, `#`, `_`, `&`, `*`, `.`), so byte and code-unit positions agree at every consumed boundary. Single-character argument reads consume one Unicode scalar instead of one UTF-16 unit; the resulting concatenated output is identical, verified by the differential corpus (`😀`, CJK, combining marks).
- `Array.from(value)` code-point counting is reproduced with `chars().count()`; `String.prototype.trim`/`trimEnd` use the ECMAScript whitespace set from `js_compat`.
- `\p{L}` / `\p{N}` are compiled by `regex` (Unicode general categories), matching the JavaScript property escapes. `\b` in the case-condition pattern is implemented as an ASCII word boundary (`has_case_prefix`), matching JavaScript's non-Unicode `\b`; Rust's `\b` is Unicode-aware and would differ for `ifé`.
- The source's `SYMBOLS`, `NEGATED_SYMBOLS`, `BLACKBOARD`, `SUPERSCRIPTS`, `SUBSCRIPTS`, `ACCENTS`, and command sets were extracted mechanically from the TypeScript file, not retyped.

## Gaps / exclusions

- `RenderLatexOptions.display` is a `bool`; JavaScript callers passing non-boolean values are excluded by the typed API (decision 2026-08-12). The source treats only `display === true` as enabled, so the mapping is exact.
- No other runtime behavior is excluded.

## Discrepancies

None found. The source's unused-looking tables are both reachable (`ACCENTS` in the accent branch, `BLACKBOARD` in `\mathbb`), and their behavior is preserved.
