# autocomplete.ts traceability

Source: `packages/tui/src/autocomplete.ts` (793 lines). Rust: `crates/pi-tui/src/autocomplete.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface AutocompleteItem` | `struct AutocompleteItem { value, label, description: Option<String> }` | `tests/autocomplete.rs` (21 tests); `tests/differential_autocomplete.rs` |
| `interface SlashCommand` | `struct SlashCommand { name, description, argument_hint, get_argument_completions }` | slash-command tests |
| `interface AutocompleteSuggestions` | `struct AutocompleteSuggestions { items, prefix }` | all suggestion tests |
| `interface AutocompleteProvider` | inherent methods on `CombinedAutocompleteProvider` | all tests |
| `class CombinedAutocompleteProvider` | `struct CombinedAutocompleteProvider` + `new`/`get_suggestions`/`apply_completion`/`should_trigger_file_completion` | `tests/autocomplete.rs`; differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `getSuggestions` reads `lines[cursorLine] \|\| ""` | `lines.get(cursor_line)` | differential empty line |
| `@` prefix extraction (quoted `@"..."` and token start) | `extract_at_prefix`, `extract_quoted_prefix`, `is_token_start` | quoted `@` tests, differential `@"my folder/"` |
| fuzzy file suggestions, `null` when empty | `get_fuzzy_file_suggestions` | fd tests, differential |
| slash command names (`spaceIndex === -1`) | `command_name_suggestions` | `/mod` tests |
| command-name description/hint composition (`hint — desc`) | `CommandItem` mapping | differential `/model`, `/mod`, `help` item |
| `fuzzyFilter` over command names | `fuzzy_filter` | differential command cases |
| command argument completions; empty array returns `null` without fallback | `CommandEntry::Command` callback branch | `slash_command_argument_completions...`, `empty_argument_completions...`, differential `/cd` |
| non-forced slash-argument text returns `null` | `if !options.force { return None }` | differential `/model arg` force on/off |
| forced absolute path after command argument | `extract_path_prefix(force)` | `absolute_paths_trigger...`, differential `/command /` |
| natural path triggers (`/`, `.`, `~/`, trailing space) | `extract_path_prefix` | differential `@.`, `./up`, `my` |
| `extractQuotedPrefix` unclosed-quote scan | `find_unclosed_quote_start` | quoted tests |
| path delimiter set `space tab " ' =` | `PATH_DELIMITERS` | quoted/space tests |
| `parsePathPrefix` `@"`, `"`, `@` forms | `parse_path_prefix` | quoted tests |
| `buildCompletionValue` quoting for spaces | `build_completion_value` | `"my folder"` tests, differential |
| root prefix detection and directory/file split | `get_file_suggestions` | `./up`, `./sr`, `my`, `hey /` |
| `readdirSync` failure returns `[]` | `fs::read_dir` error path | differential `/A` |
| OS filename bytes decode at readdir ingress | shared `pi_js_compat::node_fs::decode_node_path`; no native-JS-string projection | `raw_autocomplete::readdir_os_bytes_match_node_filename_decoding`: four real POSIX names (invalid/truncated UTF-8 and supplementary scalar) versus frozen source; baseline already GREEN, authority consolidation has no intended behavior delta |
| directory and symlink-to-directory detection | `file_type` + `metadata` follow | symlink tests |
| `./`, `~/`, absolute and relative completion paths | `relative_completion_path` | `./` tests, `~/` test, differential |
| directories-first + `localeCompare` sort | `compare_suggestions`, `locale_compare` | mixed-case fixture (`README.md`, `apple.txt`, `Zebra.txt`) differential |
| `expandHomePath` (`~/`, `~`) | `expand_home_path` + `home_dir` | home expansion test |
| scoped fuzzy query (`resolveScopedFuzzyQuery`) | `resolve_scoped_fuzzy_query` | `@../outside/a` tests |
| `scopedPathForDisplay` root vs base | `scoped_path_for_display` | scoped differential |
| fd argument order, `.gitignore`/`--ignore-file`, `--full-path`, path query | `walk_directory_with_fd`, `build_fd_path_query` | fd tests; differential runs the real fd |
| fd abort kills the child / late abort while pipe remains open | shared AbortSignal + kill_on_drop/start_kill; public fuzzy consumer rechecks aborted after walker completion | `autocomplete_close_abort`: actual fake-fd parent exits0 while its child holds stdout; prove parent reaped, abort, then release pipe; frozen/native baseline yields one item and aborted case yields none |
| fd output cleanup and `.git` exclusion | `walk_directory_with_fd` | hidden-path test |
| `scoreEntry` weights and directory bonus | `score_entry` | fuzzy ranking tests, differential |
| stable descending score sort and top 20 | `sort_by` + `truncate(20)` | differential |
| `applyCompletion` quoted-after-cursor adjustment | `adjusted_after_cursor` | quoted apply tests, differential |
| slash command application (`/name `) | `is_slash_command` branch | `apply_completion_for_slash_commands` |
| `@` application (space after files, none after directories) | `prefix.starts_with('@')` branch | apply tests, differential |
| command-argument vs plain file application cursor | trailing branches | differential `/cd s`, `src` |
| `shouldTriggerFileCompletion` | `should_trigger_file_completion` | trigger test |

## Behavioral notes

- Cursor positions are UTF-8 byte offsets. The oracle converts them to UTF-16 indices before calling TypeScript and converts results back, so both sides exercise the same boundaries.
- fd is spawned with the exact argument vector and order from the source: `--base-directory`, `--max-results 100`, `--type f`, `--type d`, `--follow`, `--hidden`, `--no-ignore-vcs`, three `--exclude` entries, optional `--ignore-file <base>/.gitignore`, optional `--full-path`, optional path query. Differential runs the real `fd` for both implementations against one fixture tree.
- Node path semantics are reproduced by local POSIX `normalize`/`join`/`dirname`/`basename` ports, including `./` preservation and `~` display paths.
- Empty `description` is omitted (`None`), matching `...(item.description && {description})` and `fullDesc || undefined`.
- fd stdout is decoded lossily from UTF-8 and trimmed with the ECMAScript whitespace set, matching `setEncoding("utf-8")` and `String.prototype.trim`.

## Gaps / exclusions

- `SlashCommand.getArgumentCompletions` may return `Awaitable<...>` in TypeScript. The Rust callback is synchronous: `Fn(&str) -> Option<Vec<AutocompleteItem>>`. Async argument completion providers are excluded by the typed API (decision 2026-08-12). No caller in the ported tests uses an async callback.
- `localeCompare` uses the runtime's default locale (Node here: `en-US`). `locale_compare` reproduces its primary case-insensitive ordering and lowercase-before-uppercase tie-break for ASCII names; diacritics and other locale-specific collation weights are not reproduced. The differential fixture asserts mixed-case ASCII ordering only.
- `os.homedir()` falls back to the passwd database when `HOME` is unset. `home_dir` reads `HOME` and yields `""` when unset; no passwd fallback.
- Directory entry names are read as lossy UTF-8; JavaScript strings from `readdirSync` can represent any byte sequence.
- `applyCompletion` with an out-of-range `cursorLine`: TypeScript assigns `newLines[cursorLine]`, extending the array with holes. Rust leaves the line vector unchanged; the typed API requires a valid line index.
- Abort behavior (`signal.aborted` checks and SIGKILL) is implemented with `CancellationToken` but is not exercised by the differential test because it depends on process timing. Unit tests cover the non-aborted path.

## Discrepancies

None found. Sorting is the only intentional approximation, recorded above as a gap because the source itself delegates to locale-dependent `Intl.Collator`.

## Homedir throw/catch parity (2026-10-23)

`expand_home_path` and `resolve_scoped_fuzzy_query` return NativeNodeError through Result. Frozen getFileSuggestions and getFuzzyFileSuggestions catches consume those failures and return no suggestions. `differential_autocomplete::nss_home` forces getpwuid_r EIO in isolated source/native processes for `~/` and `@~/`; both return null without a panic. RED/GREEN logs: `/tmp/pi-rust-repin-lossless-interactive/autocomplete-home-{red,green}.log`.

## Harness intermittent (acceptance 2026-10-26)

`autocomplete_close_abort::abort_after_parent_exit_but_before_stdout_close_matches_frozen` failed once in the full workspace run because the frozen oracle side reported "fd parent did not exit before abort" (its own setup deadline under load), then passed 5/5 targeted reruns. Scheduling/timing gap category; native behavior was not the failing side.
