# config traceability

Historical source: `packages/coding-agent/src/config.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/config.rs`.

## Frozen-version checkpoint

Current reference: immutable `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Embedded coding-agent package version is `0.80.3`, matching that source's package
manifest. `differential_config::advertised_version_matches_frozen_package`
compares the public runtime version to the actual frozen manifest (RED at
`0.84.1`). Cargo crate version remains independent. Other embedded manifest
fields and config behavior remain unvalidated here; this is not whole-config
parity or a byte-identical manifest claim. Evidence below is historical unless
explicitly revalidated.

Differential proof: `oracle/config-oracle.ts` executes the pinned module with a controlled
`process.env`, `process.execPath`, `process.argv[1]`, and `process.platform`, and
`crates/pi-coding-agent/tests/differential_config.rs` compares four path scenarios and thirteen
install scenarios byte-for-byte. Focused tests: `crates/pi-coding-agent/tests/config_paths.rs`
(7 cases, child processes for environment isolation) and `crates/pi-coding-agent/tests/config_install.rs`
(16 cases, ports of every `test/config.test.ts` case plus `PNPM_HOME` and runtime-layout cases).

## Runtime mapping (approved 2026-10-18)

The Rust port is a native executable, so it takes the TypeScript compiled-binary (`isBunBinary`)
branch everywhere:

- Embedded assets (approved deviation 2026-10-18): without `PI_PACKAGE_DIR`, `package.json` and the built-in `dark`/`light` themes are byte copies of the pinned source compiled into the binary (`embedded_builtin_theme`, `PackageConfig::parse`); `PI_PACKAGE_DIR` restores disk reads. Tests: `main_process::embedded_assets_version_differential`, `print_mode_with_embedded_assets_differential`, `embedded_assets_are_byte_copies_of_the_pinned_source`.
- `getPackageDir` = `PI_PACKAGE_DIR` when set, otherwise the executable's directory. The Node
  walk-up-to-`package.json` search is not ported because there is no module `__dirname`.
- `getThemesDir` = `<package>/theme`, `getExportTemplateDir` = `<package>/export-html`,
  `getInteractiveAssetsDir` = `<package>/assets`. The Node `src/`-or-`dist/` layout branch is not
  ported; `tests/config_paths.rs` asserts the compiled-binary paths, and the differential test
  excludes exactly those four keys.
- `detectInstallMethod` returns `bun-binary`, so `getSelfUpdateCommand` is `None` and the update
  instruction is the release download URL.
- The npm/pnpm/yarn/bun branches remain implemented as pure functions over `InstallContext` (package
  dir, exec path, argv entrypoint, platform, home, `PATH`, environment), which is what the
  differential test drives with `is_bun_binary = false`, mirroring Node's runtime facts.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `config.ts:17-21` | Compiled-binary/runtime detection | `InstallContext::{is_bun_binary,is_bun_runtime}`; `the_rust_binary_behaves_like_the_compiled_typescript_binary` |
| `config.ts:56-60` | Install step display quotes arguments containing whitespace | `make_step`; `quotes_npm_self_update_display_paths_with_spaces` + differential |
| `config.ts:62-79` | Renamed packages uninstall first and chain displays with ` && ` | `make_command`; npm/pnpm/yarn/bun renamed tests + differential |
| `config.ts:82-100` | `detectInstallMethod` path probes (pnpm, yarn, bun, npm) | `detect_install_method`; `detects_*` tests + differential |
| `config.ts:104-124` | npm prefix inference (`<prefix>/lib/node_modules/<pkg>`) and Windows opt-out | `get_inferred_npm_install`; `self_updates_npm_installs_from_custom_prefixes`, `does_not_infer_windows_npm_custom_prefixes` + differential |
| `config.ts:126-215` | Per-method commands, install specs, `--prefix`, pnpm `global-bin-dir` | `get_self_update_command_for_method`; 16 focused install cases + differential |
| `config.ts:217-260` | Command probes (`pnpm root -g`, `yarn global dir`, `bun pm bin -g`, npm/bun `root -g`), `requireSuccess` errors | `read_command_output`, `get_global_package_roots`; fake-script tests + differential |
| `config.ts:262-303` | Global-root containment check with symlink and case normalization | `is_managed_by_global_package_manager`, `path_comparison_candidates`; unwritable/unknown tests + differential |
| `config.ts:305-345` | `getSelfUpdateCommand`, `getUpdateInstruction`, unavailable instructions | same-named functions; `does_not_self_update_*`, `the_rust_binary_*` + differential |
| `config.ts:347-420` | Package asset paths and `PI_PACKAGE_DIR` override | `get_*_dir`, `get_*_path`; `package_assets_resolve_from_the_package_dir`, differential path scenarios |
| `config.ts:422-478` | `package.json` constants, `piConfig` overrides, defaults, missing-file tolerance, non-ENOENT/parse throws | `PackageConfig::load`, `try_package_config`, `package_config`; `package_constants_come_from_package_json_piconfig`, `missing_package_json_uses_the_default_constants`, `invalid_package_json_surfaces_as_a_node_syntax_error`, `unreadable_package_json_surfaces_as_a_node_fs_error`, `non_string_package_fields_fall_back_to_the_defaults`, differential constants |
| `config.ts:480-500` | `expandTildePath` and share viewer URL | `expand_tilde_path`, `get_share_viewer_url`; path tests + differential |
| `config.ts:502-594` | XDG config/state roots, legacy dir, agent dir and every derived file path | `get_user_config_root`, `get_user_state_root`, `get_legacy_agent_dir`, `get_agent_dir`, `get_*_path`; `xdg_roots_*`, `home_fallbacks_*`, differential path scenarios |

## Gaps

- Node-only package discovery (`getPackageDir` walk-up, `src`/`dist` asset layout) is replaced by the
  approved compiled-binary mapping.
- Windows install layouts are exercised through injected platform/path values; the writability probe
  uses `rustix::fs::access` on Unix and the readonly bit elsewhere.
- `spawnProcessSync` error text for a missing package manager differs (Rust reports the spawn error
  instead of Node's `ENOENT` message); only surfaced through `requireSuccess` failures, which no
  ported caller triggers with a configured command.

## Discrepancies

- None observed in the compared surface: all 4 path and 13 install scenarios match the pinned module
  byte-for-byte, apart from the documented compiled-binary asset paths.

## Proof inventory

- Focused cases: 23 (7 paths, 16 install).
- Differential: 4 path scenarios, 13 install scenarios.

## Q5 native-error carrier prerequisite (2026-10-23)

ConfigError and shared PathError retain an originating NativeNodeError as their std::error::Error source, including constructor class, code, SystemError context and URL input fields. No message-only reconstruction replaces that source. Shared file-URL failures now construct the actual TypeError/URIError identities rather than generic strings. `differential_config_error_sources` compares four frozen normalizePath failures through the PathError→ConfigError conversion and checks preservation of the supplied homedir SystemError. `differential_config_error_sources` passed after parent integration. Config getters and their production consumers now propagate Result instead of panicking. `differential_config_error_boundaries` compares 18 getters across unset/explicit/empty HOME, successful/failed NSS, four malformed file URLs and a relative package directory, including original source downcasts; GREEN on 2026-10-23. `differential_read_classification` also passes the config/deleted-cwd renderer boundary. Coding-agent all-target fixture migration/check passed at `5030bd78`; the shared workspace check then found orchestrator's auth consumer, covered below. This is not whole-config closure. `process_env::get_native` is the shared OS-byte/overlay authority used by homedir, retaining Node decoding for invalid OS bytes. `pi-js-compat --test node_os` passed empty/unset/invalid-byte HOME and SystemError cases after integration. Config/read/keybindings/system-prompt regression batch: 26 passed; auth_storage's two failures reproduce on parent e43d17cc and are outside this slice.

### SDK carrier follow-up

Frozen `sdk.ts:215–246,293,440` does not catch default path/auth/model/session construction failures. SDK now carries ConfigError/PathError and SettingsFailure/SessionManagerError/AgentSessionError into CallbackError, retaining both native rejection and original Rust source instead of constructing message-only errors. `config_runtime_error_carriers` reproduces forced NSS at SDK construction (RED: plain Error; GREEN: SystemError with info.code EIO and original source). Central ConfigError uncaught routing delegates to utils/uncaught::throw_native_uncaught. The same forced-NSS regression now covers services and exported main construction. Runtime factories/transitions, main, extension UI responses, and print/RPC rebinding carry CallbackError until their actual display/response catch. `CallbackError::from_error` consolidates native-source traversal (ConfigError, path, session, settings and DB wrappers) while retaining the original Rust error; existing callback rejections retain their metadata. Print output, RPC error responses and handleFatalRuntimeError project messages only at the frozen catches (`print-mode.ts:169–171`, `rpc-mode.ts:789–795`, `interactive-mode.ts:2181–2185`). `config-runtime-carriers-oracle.mjs` loads frozen modules before forcing NSS, then compares SDK/services/main/login rejection constructor, own properties and String(error) to the native carriers. Login help constructs no auth storage (frozen login-command.ts:201–214); moved native default auth construction after that guard. The native ordering regression failed before this move; the differential suite now passes. Other consumer projection audit remains open.

### Orchestrator import boundary

Frozen `orchestrator/src/cli.ts:11` statically imports `serve.ts`, whose line 6 imports `radius.ts`; `radius.ts:119` constructs AuthStorage during module evaluation. Its native configuration throw precedes main dispatch, including --version, and is not caught by the later serve recovery catch (`serve.ts:29–34`, which rethrows unchanged anyway). Native `run_cli` initializes RadiusOptions before execute_cli/parse_args and routes that uncaught failure through ConfigError::raise_uncaught. The already-initialized options are passed to serve rather than constructing auth twice. Embedded start_default/serve return CallbackError retaining the original ConfigError/native rejection. `pi-orchestrator --test config_error_propagation` forces NSS EIO in real children, checks the embedded rejection/source, and compares frozen/native --version import failure (exit 1, native SystemError prefix, no version output). Existing CLI differential/process tests also pass (5 passed total, two child entry points ignored by the runner).
