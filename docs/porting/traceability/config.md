# config — common-pin closure

Baseline gap fixture (2026-10-25): the NUL application-name case asserts native override `/environment-agent` separately from frozen's default agent directory; Node truncates environment names at NUL (approved runtime-oddity gap). Every remaining field, including that default directory, and every realistic package case remain exact differentials.

Source: `packages/coding-agent/src/config.ts`, immutable `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d` (602 lines). Rust: `crates/pi-coding-agent-base/src/config.rs`. Verified: 2026-10-23.

Status: **done within the explicit runtime mapping and typed-input contract below**. Historical `532f7d12` evidence is not substituted for these common-pin proofs. Compiler/lint milestone gates remain separate from source coverage.

## Symbol and branch map

| Frozen lines / symbols | Exact branch coverage and Rust mapping | Proof |
|---|---|---|
| 12–23 `__filename`, `__dirname`, `isBunBinary`, `isBunRuntime` | Native process facts in InstallContext; compiled-binary mapping; injected Node/Bun install facts | `config_install`; actual frozen virtual-Bun-URL metadata/layout differential |
| 29–41 InstallMethod, SelfUpdateCommandStep/Command, SelfUpdatePackageTarget | Native enum, owned command/optional steps, target constructors; omitted installSpec defaults to packageName, explicit spec retained | `config_install`, `differential_config` |
| 43–51 `normalizeSelfUpdatePackageTarget` | String target/default target and object installSpec represented by target constructors | Same install matrices, exact args/display |
| 53–63 `makeSelfUpdateCommand` | Single step omits steps; renamed package emits uninstall then install and joined display | Renamed npm/pnpm/yarn/bun tests and frozen differential |
| 65–71 `makeSelfUpdateCommandStep` | JavaScript whitespace quoting, exact argument order and joined display; JsString throughout | Space-path tests and install differential |
| 73–94 `detectInstallMethod` | Compiled short circuit; pnpm/.pnpm, yarn, Bun runtime/global shape, npm/node_modules, unknown | `config_install` plus 13 frozen install scenarios |
| 96–113 `getInferredNpmInstall` | Scoped/unscoped node_modules, lib prefix, no prefix otherwise; POSIX backslash paths deliberately select win32 dirname/basename | `config_install`; 12 frozen shape controls including drive-relative, UNC, Unicode, mixed/repeated separators |
| 115–187 `getSelfUpdateCommandForMethod` | bun-binary/unknown undefined; pnpm global-bin inference and PNPM_HOME; yarn/bun/npm arguments; optional npm prefix; exact/renamed targets | `config_install`, `differential_config`, install-path differential |
| 189–204 `readCommandOutput` | Validate file/argv before encoding; successful trimmed/empty stdout; ignored failed probe; required spawn/stderr/status failure | Four actual-frozen invalid-argument cases; configured-command tests; shared Node differential |
| 206–249 `getGlobalPackageRoots` | Configured npm, Bun-as-npm, default npm/inference; pnpm root/parent or regex fallback; yarn roots; Bun home/bin roots; empty unknown/binary | Install matrix, fake local managers, frozen differential |
| 251–268 `normalizeExistingPathForComparison` | Lazy cwd only for relative paths; missing path; canonicalize success/failure; POSIX path spelling | Real removed-cwd process and install matrix; Windows lowercase is a gap below |
| 270–278 `getPathComparisonCandidates` | Lexical/canonical candidates, undefined filtering and ordered deduplication | Managed/unmanaged/unknown-wrapper install cases and real process matrix |
| 280–291 `getEntrypointPackageDir` | Missing/empty entrypoint; nearest package.json; stop before root; POSIX backslash is a filename character | Actual frozen/native backslash-entrypoint process plus wrapper install cases |
| 293–302 `isSelfUpdatePathWritable` | Package and Node dirname parent both writable; failure returns false; relative single-component parent is dot | Actual frozen/native relative-package process; non-writable tests |
| 304–313 `isManagedByGlobalPackageManager` | Package and entrypoint candidates; global roots; separator-terminated containment, no sibling-prefix match | Fake managers and actual process/path matrices |
| 315–326 `getSelfUpdateCommand` | Undefined command/management/writability gates in source order; command retained only when all succeed | Install differential and focused branch tests |
| 328–346 `getSelfUpdateUnavailableInstruction` | Binary release URL; managed-but-unwritable; unmanaged method; unknown target guidance | `config_install`, frozen install differential |
| 348–355 `getUpdateInstruction` | Command display or unavailable instruction | Exact frozen displays, including 12 inference shape controls |
| 367–387 `getPackageDir` | Nonempty PI_PACKAGE_DIR normalization or compiled executable directory; native result stays JsString | Paths matrix, native error/source matrix; approved Node-layout mapping below |
| 396–420 `getThemesDir`, `getExportTemplateDir` | Compiled theme/export-html directories | Actual frozen compiled-layout fixture comparison |
| 422–444 `getPackageJsonPath`, `getReadmePath`, `getDocsPath`, `getExamplesPath`, `getChangelogPath` | Node join; four absolute resolves use lazy cwd | Paths/asset tests, current-pin error matrix |
| 452–464 `getInteractiveAssetsDir`, `getBundledInteractiveAssetPath` | Compiled assets directory, arbitrary typed string asset name joined without premature encoding | Actual frozen compiled-layout fixture comparison |
| 470–485 PackageJson/module read | Node UTF-8 file decoding, native JSON parse, ENOENT-only defaults, other read/parse throws; one stored result | Missing, EISDIR, malformed UTF-8/syntax tests; actual --version uncaught child |
| 487–492 PACKAGE_NAME, APP_NAME, APP_TITLE, CONFIG_DIR_NAME, VERSION | Five JsString fields; omitted/empty-string defaults, title pi glyph when name absent; raw UTF-16 retained | Eight actual-frozen manifest fixtures and explicit invalid-field tests |
| 495–497 ENV_AGENT_DIR/SESSION_DIR/STATE_DIR | JS uppercase then native environment-name encoding; NUL/key aliasing at shared process_env boundary; human labels derive separately | Raw surrogate/NUL fixtures, process_env Node differential, help-label comparison |
| 499–501 `expandTildePath` | Shared normalizePath, including eager default homedir failures | Paths, file-URL/native-source and NSS matrices |
| 503–509 DEFAULT_SHARE_VIEWER_URL, `getShareViewerUrl` | Nonempty env override or default, literal hash plus arbitrary JsString ID | Four frozen path scenarios; scalar and lossless getter contract |
| 516–521 `getUserConfigRoot` | XDG expansion or homedir/.config, then raw APP_NAME join | Paths/NSS matrices, manifest differential |
| 523–532 `getUserStateRoot` | Nonempty native ENV_STATE_DIR first; XDG expansion or homedir/.local/state; APP_NAME join | Paths/override/NSS matrices and manifest differential |
| 534–539 `getUserCacheRoot` | XDG_CACHE_HOME expansion or homedir/.cache, APP_NAME join; normalization's native failures propagate | Cache RED/GREEN, 19-getter matrix, raw manifest fixtures |
| 541–550 `getLegacyAgentDir`, `getAgentDir` | Legacy CONFIG_DIR_NAME join; nonempty agent override or user root/agent | Paths, override/native-error matrices and raw manifest fixtures |
| 555–597 `getCustomThemesDir`, `getModelsPath`, `getAuthPath`, `getSettingsPath`, `getToolsDir`, `getBinDir`, `getPromptsDir`, `getPromptHistoryPath`, `getSessionsDir` | Same original JsString getters, Node joining, inherited errors | Full paths payload plus 19-getter native-source matrix |
| 600–602 `getDebugLogPath` | Raw APP_NAME-debug.log joined to agent root | Actual frozen raw manifest/root comparison |

## Accepted mappings and exclusions

- **Native executable mapping:** use frozen compiled-binary branches. Node package-root walking and src/dist asset selection are replaced, not fallback paths. The actual frozen predicate is exercised with a virtual `~BUN` module URL, without changing frozen bytes. Eight manifest fixtures compare the resulting flags, method and all compiled asset layouts.
- **Embedded assets:** without PI_PACKAGE_DIR, pinned package.json and dark/light themes are embedded; an override reads disk. Embedded package version is 0.80.3, independent of Cargo version. Existing `main_process` embedded-byte/version tests cover the binary packaging contract.
- **Typed manifest exclusion (2026-10-23):** package.json is build-controlled. Wrong known-field types are excluded, but Rust now fails explicitly instead of silently defaulting. Frozen `config.ts:495` reaches `APP_NAME.toUpperCase()` for a truthy array name and throws during evaluation. Native ConfigError uses that module uncaught boundary. Missing fields and valid empty strings retain source defaults; the old non-string defaulting test was deleted.
- **Native environment names:** env_* returns native String keys under the project rule, not human display strings. Help pads the raw metadata-derived label in UTF-16 before terminal encoding. Other metadata, returned paths/text and errors stay lossless.
- Invalid JavaScript runtime types in typed core APIs, JS object identity, and V8 stack text retain the explicit project exclusions; stack is omitted, never fabricated.

## Windows-platform gaps

| Frozen branch | Classification |
|---|---|
| 98 process.platform win32 choice in inferred npm layout | gap for actual Windows runtime; POSIX contains-backslash selection is implemented and differentially proved |
| 263–265 Windows comparison lowercasing/path behavior | gap |
| 293–302 Windows access/writability behavior | gap; POSIX access matches Node |
| Imported platform-default Windows dirname/join/resolve and process transport | gap under the project Windows-platform exclusion |

No valid-domain source bugs were reclassified or silently fixed. Six native path defects and the synchronous spawn-validation defect were fixed against failing real-source/process counterexamples.

## Consumer and error boundaries

ConfigError/PathError preserve NativeNodeError constructor, code, errno/syscall, SystemError info and URL input through original Rust sources. CallbackError conversion has an explicit JsString message, no Display fallback. Runtime/SDK/services/CLI/architect/supervisor/relay carry that rejection until a frozen catch; module evaluation uses the shared uncaught authority. Real frozen/native NSS tests cover twelve constructor/command boundaries, login-help's no-auth guard, selector current/all catches, and orchestrator --version's static Radius AuthStorage import. See [native error classes](native-error-classes.md), [SDK](sdk.md), [interactive mode](interactive-mode.md), [orchestrator CLI](orchestrator-cli.md).

Lossless consumers include auth/default paths and guidance, slash descriptions, help labels, theme paths, shell PATH, managed tool paths, relay versions and extension text. Native filesystem/process/terminal interfaces encode; HTTP headers use shared ByteString validation. [Shared spawn argument proof](node-child-process-arguments.md) covers boxed native rejections, harness cwd/env labels and editor delegation.

## Proof ledger

Lane artifacts: `/tmp/pi-rust-repin-core-tools/`.

| Scope / revision | Evidence |
|---|---|
| acbf2209 metadata/result consumer checkpoint | `config-metadata-compile-7.jsonl`: workspace all-target 0/0; `config-metadata-focused-1.log`: 33/0 |
| 8982ed79 manifest rejection and spawn integration | `config-package-field-red.log` and `config-spawn-validation-red-4.log` RED; `config-input-boundaries-green.log`: 13/0; all-target 0/0 |
| eb1e0b59 install path corrections | `config-install-paths-red-3.log` six RED counterexamples; `config-install-paths-green.log`: 20/0, including three actual-process and 12 shape controls |
| 01868b26 compiled layouts/defaults | `config-compiled-layouts-green.log`: eight actual-frozen fixtures GREEN; all-target 0/0 |
| 43191427 + parent merge6101e5b3 boxed authority | Shared Node, harness spawn/env and config probe differentials GREEN; `config-closure-workspace.jsonl`: workspace all-target 0/0 |

Later documentation changes do not invalidate those scopes. No tests are claimed for unchanged portions merely because a new milestone occurred. Final clippy remains blocked by the 33 pre-existing pi-js-compat findings, explicitly deferred by the parent; the two newly introduced large-Err findings were fixed with boxed rejections, not suppressions. Full workspace clippy/readability/independent milestone acceptance is not claimed by this source row.
