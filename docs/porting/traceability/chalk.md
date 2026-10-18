# chalk dependency traceability

Pinned source: `node_modules/chalk` 5.6.2 (as pinned by `packages/coding-agent/package.json`) at
`532f7d12cd19235081bd4d6e425bde185652d86f`. Files read: `source/index.js`, `source/utilities.js`,
`source/vendor/ansi-styles/index.js`, `source/vendor/supports-color/index.js`.

Rust port: `crates/pi-coding-agent/src/utils/chalk.rs`.

Differential proof:

- Request `26100` runs the real `Chalk` class at levels 0-3 over 1872 generated cases
  (4 levels x 18 style chains x 26 texts: empty, plain, spaces, tabs, `\n`, `\r\n`, mixed
  newlines, trailing newline, lone `\r`, unicode, ESC-only, and strings containing the styles'
  own close codes).
- `oracle/chalk-detect-oracle.mjs` imports the pinned `createSupportsColor` in a child node
  process per environment/argv scenario and reports the levels for `isTTY` true and false;
  76 scenarios x 2 TTY states are compared with `detect_level_for`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `index.js` `applyOptions` | Level is an integer 0-3; invalid levels throw | `Level` enum (`Plain`/`Basic`/`Ansi256`/`Ansi16m`), `Level::number`, `Level::from_number`; typed API excludes invalid levels |
| `index.js` `createStyler` | `openAll` = parent opens then own open; `closeAll` = own close then parent closes | `Chalk::paint` builds `open` in order and `close` in reverse order; `chained_styles_nest_open_and_close_codes`, differential 26100 |
| `index.js` `applyStyle` level guard | Level 0 returns the input untouched | `plain_level_emits_no_codes`, differential 26100 (level 0) |
| `index.js` `applyStyle` empty guard | Empty string returns `""` with no codes | `empty_text_stays_empty`, differential 26100 |
| `index.js` `applyStyle` nested-close loop | When the text contains ESC, each style replaces its close code with `close + open` from innermost to outermost | `reopen_embedded_closes`; `embedded_close_code_reopens_the_style`, differential 26100 |
| `utilities.js` `stringEncaseCRLFWithFirstIndex` | Close before and re-open after every `\n`; a preceding `\r` stays inside the escaped line break | `encase_lines`; `newlines_close_before_and_reopen_after`, differential 26100 |
| `index.js` `createBuilder` | Single-argument builder applies the style chain | `Chalk::paint(&[Style], &str)` plus per-style helpers (`red`, `dim`, `yellow`, `bold`, `green`, `cyan`, `italic`, `strikethrough`, `blue`, `underline`, `gray`, `magenta`, `inverse`); `named_style_helpers_match_paint` |
| `vendor/ansi-styles/index.js` `styles` | Exact open/close SGR codes for modifiers and foreground colors; `gray`/`grey` alias `blackBright` (90) | `Style::{open, close}` (9 modifiers, 16 foreground colors + `Gray`); `basic_style_codes_match_ansi_styles`, differential 26100 |
| `vendor/supports-color/index.js` `hasFlag` | Flag lookup with `-`/`--` prefix and `--` terminator cutoff | `has_flag`; `level_detection_honors_color_argv_flags`, differential scenarios `flag *` |
| `vendor/supports-color/index.js` `flagForceColor` | `--no-color(s)`, `--color=false/never` force 0; `--color(s)`, `--color=true/always` force 1 | `argv_force_color`; differential scenarios `flag color*`, `flag no color*` |
| `vendor/supports-color/index.js` `envForceColor` | `FORCE_COLOR`: `true`/empty => 1, `false` => 0, else `min(parseInt, 3)`; overrides argv | `env_force_color`, `ForceColor`; differential scenarios `force *` |
| `vendor/supports-color/index.js` `_supportsColor` order | force 0 short-circuit, then `--color=16m/full/truecolor` => 3, `--color=256` => 2, Azure `TF_BUILD`+`AGENT_NAME` => 1, non-TTY without force => 0 | `detect_level_for`, `argv_level_override`; differential scenarios `force zero beats 16m flag`, `azure *`, every scenario's `noTty` column |
| `_supportsColor` TERM handling | `TERM=dumb` => forced minimum; `-256(color)?$` => 2; `^screen\|^xterm\|^vt100\|^vt220\|^rxvt\|color\|ansi\|cygwin\|linux` => 1 | `detect_level_for`, `term_name_level`, `term_256_regex`, `term_basic_regex`; differential scenarios `term *`, `dumb with truecolor` |
| `_supportsColor` CI handling | `CI` with GitHub/Gitea/Circle => 3; Travis/AppVeyor/GitLab/Buildkite/Drone or `CI_NAME=codeship` => 1; otherwise forced minimum | `ci_level`; differential scenarios `ci *`, `github actions`, `gitea actions`, `circleci`, `codeship` |
| `_supportsColor` TEAMCITY | `^(9\.(0*[1-9]\d*)\.\|\d{2,}\.)` => 1, else 0 | `teamcity_level`, `teamcity_version_regex`; differential scenarios `teamcity *` |
| `_supportsColor` terminal capabilities | `COLORTERM=truecolor`, `TERM=xterm-kitty`/`xterm-ghostty`/`wezterm` => 3; `TERM_PROGRAM=iTerm.app` version >= 3 => 3 else 2, `Apple_Terminal` => 2; any `COLORTERM` => 1 | `terminal_level`, `terminal_program_level`, `iterm_level`, `term_name_level`; differential scenarios `colorterm *`, `term kitty`, `term ghostty`, `term wezterm`, `iterm *`, `apple terminal`, `other terminal program` |
| `index.js` `chalk` / `chalkStderr` | Default instance uses the stdout level, `chalkStderr` the stderr level | `Chalk::stdout` / `Chalk::stderr` via `detect_level(Stream)` using `std::io::IsTerminal` |
| `NO_COLOR` | Not read by supports-color 5.6.2 | No branch; differential scenarios `no color env`, `empty no color env` prove the variable is ignored |

## Discrepancies

- No source bugs found.
- chalk levels outside `0..=3` (and `NaN`/negative `FORCE_COLOR` results below 0) are excluded by
  the typed `Level` API. JavaScript clamps those to "no styling" at use time; `level_from_min`
  maps every value <= 0 to `Level::Plain`, so the observable output is identical.
- `parse_int` saturates at `i64::MAX`/`i64::MIN` while `Number.parseInt` yields a double; every
  consumer compares the result against `0`/`1`/`2`/`3` (or `>= 3`), so both give the same level.

## Gaps

- Background colors (`bgColor`), `rgb`/`hex`/`ansi256` color models, `visible`/`IS_EMPTY`, and
  multi-argument builder calls are not ported: no Pi source uses them. Multi-argument calls are
  an invalid-input exclusion (typed `&str`).
- The `process.platform === "win32"` `os.release()` branch is not ported; this workspace targets
  Linux, and the branch cannot be exercised on the current platform.
- Non-UTF-8 environment values are decoded lossily (`vars_os` + `to_string_lossy`); JavaScript
  process.env values are UTF-8 strings, so this only affects inputs JavaScript cannot represent
  faithfully either.
- chalk caches the argv-derived force flag at module load; `detect_level` re-reads argv per call.
  Observable output is identical for a single process invocation.

## Proof inventory

- Focused cases: 10 in `crates/pi-coding-agent/tests/chalk.rs`, 2 in
  `crates/pi-coding-agent/tests/deprecation.rs`.
- Differential: request `26100` (1872 styling cases) and 76 environment/argv scenarios x 2 TTY
  states for level detection.
