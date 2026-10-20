# `coding-agent/examples/extensions/commands.ts`

Pinned runtime source: `532f7d12cd19235081bd4d6e425bde185652d86f`.
Native source: `crates/pi-coding-agent/examples/extensions/commands.rs`.
Public entry: `commands_extension(&ExtensionApi) -> Result<(), HandlerError>`.
Native adaptation: explicit source embedding and `ExtensionFactory`/registry;
never autoloaded, no standalone Cargo target or library export.

## Symbol / branch map

All rows use `pinned_commands_example_matches_metadata_completions_and_all_actions`
in `crates/pi-coding-agent/tests/example_commands.rs`. The test loads only this
factory and compiles the actual example file; pinned TS is imported directly by
`oracle/example-commands-oracle.ts`. Complete values compare without normalization.

| Source symbol/branch | Concrete proof |
| --- | --- |
| default registration | One native command; exact name/description/completion and handler presence match TS registration metadata |
| `getArgumentCompletions` | Empty prefix preserves extension/prompt/skill order; partial/full prefix; nonmatching, uppercase, whitespace, non-BMP prefix return null |
| `handler`: command snapshot | Same handler receives changing bound action command lists, including empty and late-added command; each action transcript begins with getCommands |
| trim/filter/empty | Empty filter, three sources, BOM/NBSP/U+2028, non-trim U+0085/U+200B, arbitrary/unknown/plural/case-sensitive filter; exact empty messages |
| `formatCommand` | Missing, empty, nonempty, separator-containing and newline/non-BMP descriptions; exact `/name` labels |
| grouped source loop | Interleaved fixtures; fixed headers and original within-group order; exact selector title/items |
| selection guards | Cancel, empty, all three headers, unknown selected name |
| selected name extraction | Separator in name resolves first segment; leading slash retained after removing exactly one code unit; duplicate name selects first original outside filtered group |
| source path / confirm | Empty path, truthy Unicode path, confirm yes/no; exact title/prompt and conditional info notification |

## Source oddities retained

The source usage comment says `extensions`; the accepted source key is singular
`extension`. Plural input emits `No extensions commands found`. Duplicate names
resolve against the entire original command list, not the filtered group.
` - ` inside a name truncates that name before lookup. These are preserved source
behaviors, not validation improvements.

## Boundaries and remaining proof

- Native `String`/enums exclude invalid JS types, unpaired surrogates, and invalid
  source tags. Scalar Unicode fixture values cross JSON unchanged; oracle uses
  ASCII-LF framing because Node readline also splits literal U+2028/U+2029.
- No bundled hooks, real AgentSession, default CLI startup, live config/DB or
  network. Context uses an in-memory session manager, credentials/models and
  explicitly bound command-list action; models.json loading/network disabled.
- Local RED/GREEN/format ledger: `/tmp/pi-rust-example-commands-proof`.
- Coverage remains **partial** until parent's independent whole-file proof and
  final gate. No check/clippy/readability/broad/final acceptance run in this slice.
