# Coding-agent paths

Frozen source: `packages/coding-agent/src/utils/paths.ts` at `4f9e6f8a4` (118 lines). Native implementation: `crates/pi-coding-agent-base/src/utils/paths.rs`. [Node file-URL contract](../../specs/node-file-url.md).

## Export and branch map

| Frozen source | Native behavior | Current-pin proof |
|---|---|---|
| `PathInputOptions:9–20` | Owned native UTF-16 home; trim/Unicode spaces/one @/default-on tilde options | `paths` options tests; `differential_paths`; `lossless_paths` native-home matrix |
| `canonicalizePath:28–34` | Real filesystem walk; source catch returns exact original input on any failure | `differential_paths_boundaries::canonical_paths_follow_real_files_symlink_chains_and_source_catch_boundaries`:18 real file/directory/relative/absolute/chained/dangling/cyclic/raw-byte/socket/NUL/empty/root histories; `lossless_paths` four existing replacement-name/symlink histories |
| `isLocalPath:41–55` | JS trim; exact case-sensitive npm/git/github/http/https/ssh exclusions | `paths`, `differential_paths`, `lossless_paths`; ordinary names, file URLs, uppercase protocols and raw units |
| `normalizePath:57–79` | Exact order: optional trim, Unicode spaces, one @, eager home lookup when expansion enabled, bare~/slash expansion, literal file:// conversion | `paths`, common-pin `differential_paths`, native-unit `lossless_paths`; HOME-unset and forced NSS failure process proof in `differential_render_home` ([shared home authority](node-os.md)) |
| `resolvePath:81–85` | Normalize input before default-option base normalization; absolute/relative POSIX resolve | Common-pin `differential_paths`, `lossless_paths` wrapper matrix and Node POSIX matrix; deleted-cwd propagation covered by `differential_read_classification` and `differential_render_home` |
| `getCwdRelativePath:87–96` | Explicit cwd lookup; resolved cwd/path; reject .., ../ and absolute escape; empty result becomes . | `paths`, `differential_paths`, `lossless_paths`: descendant/same/ancestor/sibling/dot-prefix/raw-unit paths |
| `formatPathRelativeToCwdOrAbsolute:98–101` | Relative within cwd, otherwise absolute; POSIX slash output | Same common-pin matrices; absolute and relative raw-unit output |
| `markPathIgnoredByCloudSync:103–118` | Linux setfattr exact argv; ignored stdio and returned spawn errors/status; synchronous validation throws through Result | `differential_paths_boundaries`: actual-source/native NUL TypeError; real private subprocess exit23, missing executable and ENOEXEC. Exact argv/stdin EOF/raw-unit UTF-8 conversion; no child stdout/stderr leakage. Frozen package-manager caller1893 / native `ensure_npm_project` mechanically propagates original native error |

## Shared Node file-URL authority

`pi_js_compat::node_url::FileUrl` owns file-host/path states, separators, initial drive roots, encoded dot shortening, leading empty segments, query/fragment separation, URL error serialization and strict UTF-8 percent decoding. The base caller's existing `url::Url` dependency validates/normalizes only the host. It never supplies a pathname. There is no new dependency or alternate pathname route.

`differential_paths_boundaries` compares exact parsed results and error fields with the unchanged frozen module delegating to actual Node:29 fixed URL cases and1,250 adversarial cases (24 hosts ×52 paths +2 raw-unit cases). Cases include localhost normalization, IPv4/IPv6/invalid/Unicode hosts, authorities with credentials/ports/drives, repeated slash/backslash, initial/noninitial colon/bar drives, dot encodings, %3A/%7C round trips, malformed UTF-8/percent escapes, encoded slash precedence, NUL, raw Unicode, query/fragment serialization and USV input errors.

The original url2.5.8 pathname parser removed leading empty segments and used obsolete drive behavior. Two attempted adapters failed new cases and were removed, not retained as fallbacks. Expanded matrix RED also proved Node's encoded-slash `input` is the serialized URL, not the raw input, and nonlocal hosts remain invalid even with a first drive path. Those behaviors are now literal, not altered as source bugs.

## Explicit gaps and exclusions

| Case | Disposition |
|---|---|
| Windows-only tilde/backslash/path/URL branches | Approved platform gap (AGENTS, 2026-10-22). POSIX behavior is proven; no Windows runtime claim. |
| macOS-only two-xattr invocation branch | Literal port through the same `spawn_sync`, inspected but not executable on Linux. Main approved this platform-proof gap2026-10-24; it does not prevent module-row closure. |
| Host IDNA verdict differences | Main permits tested differences to be recorded as gaps rather than adding IDNA. None observed in the current matrix. |
| NSS failure errno/return-code mismatch | Existing approved [node-os gap](node-os.md); success paths exact. |
| Invalid runtime option/input types | Existing typed-core exclusion. Raw UTF-16 strings remain represented exactly until the actual URL/filesystem/process boundary. |

`normalizeWindowsShellPath` and `getFileRevision` are absent from the frozen module and absent from the current Rust workspace. No old-only export or revision helper remains. No newly discovered frozen logic bug is changed or silently reclassified.

## Proof ledger

Logs and exact command/revision scope: `/tmp/pi-rust-repin-core-tools/ledger.md`.

- `paths-boundaries-red.log`: actual-source cloud NUL and leading-slash RED before fixes.
- `paths-url-adversarial-first.log`:41 exact mismatches/1,202 cases before native-host/error-input corrections. Earlier adapter logs are RED, not proof of success.
- `paths-process-filesystem-green.log` at `f876fd91`:5 tests plus the ignored subprocess helper invoked explicitly three times; all URL/real-filesystem/cloud boundaries above.
- `paths-retained-matrices.log` at `f876fd91`:15 tests across `paths`, `differential_paths`, `lossless_paths`, including96 native wrapper outcomes,5,832 Node POSIX helper outcomes and four existing raw-unit realpaths. Old standalone/historical-pin logs are not acceptance evidence.
- Final gates are recorded only after their outputs exist; this document does not claim a warning-free project lint baseline.
