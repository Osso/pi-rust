# tools-manager traceability

Pinned source: `packages/coding-agent/src/utils/tools-manager.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/utils/tools_manager.rs`.

Differential test skipped: the module shells out to `tar`/`unzip`, probes `PATH`, and downloads
archives; it is proven behaviorally at the real boundary with an injected `Fetch` and real archives
(`crates/pi-coding-agent/tests/tools_manager.rs`, 11 cases).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `tools-manager.ts:11-13` | Managed `bin/` dir, 10 s API timeout, 120 s download timeout | `get_bin_dir`, `NETWORK_TIMEOUT_MS`, `DOWNLOAD_TIMEOUT_MS` |
| `tools-manager.ts:15-21` | `PI_OFFLINE` accepts `1`, `true`, `yes` (case-insensitive) | `is_offline_value`, `ToolsManager::for_process`; `offline_mode_reports_and_never_fetches` |
| `tools-manager.ts:30-74` | `fd`/`rg` release layouts and tag prefixes | `tool_config`, `asset_name`; `asset_names_match_the_release_layouts` (10 assertions) |
| `tools-manager.ts:76-83` | `command --version` probe, stderr/stdout piped | `command_exists` |
| `tools-manager.ts:85-101` | Managed binary first, then system names (`fdfind` alias for `fd`) | `get_tool_path`; `managed_binaries_win_over_system_binaries`, `system_binaries_are_found_on_path_including_the_fd_alias` |
| `tools-manager.ts:103-119` | GitHub release lookup, strip leading `v`, 10 s budget | `get_latest_version`; `json_version_payloads_without_a_tag_are_tolerated`, full install test |
| `tools-manager.ts:121-134` | Stream the response body to the archive path | `download_file`; full install test |
| `tools-manager.ts:136-156` | Depth-first search for the binary inside the extraction dir | `find_binary_recursively`; `archives_without_the_binary_report_the_expected_name` |
| `tools-manager.ts:158-179` | Spawn failure text: error message, stderr, stdout, else exit status | `run_extraction_command`; archive failure tests |
| `tools-manager.ts:181-186` | `tar xzf <archive> -C <dir>` | `extract_tar_gz`; full install test |
| `tools-manager.ts:188-197` | Prefer `%SystemRoot%\System32\tar.exe` | `windows_tar_command` (Windows only) |
| `tools-manager.ts:199-236` | Zip extraction: Windows `tar.exe` then PowerShell; elsewhere `unzip` then `tar` | `extract_zip` (not exercised on Linux) |
| `tools-manager.ts:238-314` | Resolve version, special-case fd darwin x64 to `10.3.0`, download, extract, install, chmod, clean up | `download_tool`; full install test asserts archive/extract cleanup and `0o755`; `api_and_download_failures_report_their_status`, `unsupported_platforms_report_the_pair` |
| `tools-manager.ts:316-321` | Termux package names | `termux_package`; `android_reports_the_termux_package` |
| `tools-manager.ts:323-364` | `ensureTool`: existing binary, offline/Android message, download with silent flag and colored output | `ensure_tool`, `set_silent`, `output`; offline/Android/full install tests |

## Gaps

- Zip extraction (`unzip`, Windows `tar.exe`, PowerShell `Expand-Archive`) has no Linux fixture; the
  branch is compiled but only the `.tar.gz` path runs in tests.
- `for_process` reads `PI_OFFLINE` and the platform/architecture at construction time; tests inject
  all three through `with_options`.
- The TypeScript module captures `TOOLS_DIR` at import time. Rust computes `get_bin_dir()` when
  `for_process` is constructed, so callers that change `PI_PACKAGE_DIR` before construction observe
  the new location.

## Discrepancies

- None observed in the exercised Linux `.tar.gz` path: URLs, version parsing, extraction, install
  permissions, cleanup, and every message match the source.

## Proof inventory

- Behavioral cases: 11.
