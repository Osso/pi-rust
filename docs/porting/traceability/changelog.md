# changelog traceability

Pinned source: `packages/coding-agent/src/utils/changelog.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent-base/src/utils/changelog.rs`.

Differential proof: `oracle/changelog-oracle.ts` drives the pinned module's link normalization, parsing,
comparison, and `getNewEntries`; the Rust side repeats each case
(`crates/pi-coding-agent/tests/differential_changelog.rs`): 12 link fixtures × 2 version forms,
one parse fixture plus a missing file, 3 comparisons, and 4 `lastVersion` values. Focused test:
`parse_changelog_reads_sections_and_ignores_unversioned_headers`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `changelog.ts:4-9` | Entry shape (`major`, `minor`, `patch`, `content`) | `ChangelogEntry` |
| `changelog.ts:11-16` | Repository base, legacy repo rewrite, absolute-URL scheme detection, inline link regex | `GITHUB_REPO`, `CHANGELOG_LINK_BASE_PATH`, `normalize_changelog_link_target`; 12 differential link cases |
| `changelog.ts:18-25` | Tag normalization (`v` prefix) | `normalize_tag`; exercised by every differential link case |
| `changelog.ts:27-43` | Split fragment/query/path | `split_local_target` |
| `changelog.ts:44-47` | Backslashes become forward slashes | `normalize_path_part` |
| `changelog.ts:48-60` | Resolve relative and root-absolute paths against `packages/coding-agent`; reject escapes | `resolve_repository_path`, `posix_normalize`; differential cases include `../../../../README.md` and `/packages/...` |
| `changelog.ts:61-68` | Trailing slash or extensionless basename means a directory (`tree` route) | `is_directory_target`; differential `docs/`, `docs`, `examples/extensions/` |
| `changelog.ts:70-103` | GitHub URL construction, legacy `pi-mono` rewrite, external URLs untouched, percent-encoding | `normalize_changelog_link_target`, `encode_uri`; differential Unicode, spaces, query, and external cases |
| `changelog.ts:105-114` | Rewrite every inline Markdown link | `normalize_changelog_links`; differential 24 requests |
| Common pin `changelog.ts:122`, `readFileSync(..., "utf-8")` | Invalid UTF-8 becomes U+FFFD; valid sections survive | `changelog_replaces_invalid_utf8_without_dropping_sections`; RED `/tmp/utf8-changelog-red.log`, GREEN `/tmp/utf8-changelog-green.log` |
| `changelog.ts:116-176` | `## x.y.z` section parsing, unversioned headers reset, missing file → empty, read error warning | `parse_changelog`; differential parse + focused test |
| `changelog.ts:178-185` | Lexicographic major/minor/patch comparison | `compare_versions`; differential 3 cases |
| `changelog.ts:187-199` | Entries newer than `lastVersion`; missing/invalid parts become zero | `get_new_entries(&JsString)`; differential `0.78.5`, `0.79.0`, `0.77`, `garbage`; `unpaired_surrogate_version_parts_count_as_zero` (unexecuted: coding lib does not compile) |
| `changelog.ts:201` | Re-export `getChangelogPath` | `pub use crate::config::get_changelog_path` |

## Gaps

- `normalizeChangelogLinks` accepts `string | ChangelogEntry` in TypeScript. Rust models the union
  with `normalize_changelog_links(markdown, version)` and
  `normalize_changelog_links_for_entry(markdown, version)`; both funnel into the same implementation.
  This is the typed-input exclusion: a Rust caller cannot pass a non-string/non-entry value.

## Discrepancies

- DRIFT (RED-first after the coding lib compiles): `getNewEntries` uses `Number(part)`; Rust
  parses `u64`, so whitespace (`" 2"`), exponent (`"1e1"`), hex (`"0x2"`), negative and
  fractional parts diverge from `changelog.ts:189-195`.

## Proof inventory

- Differential: 24 link requests + 1 parse + 3 comparisons + 4 new-entry queries.
- Focused cases: 1 (multi-section parse, unversioned reset, missing file via differential).
