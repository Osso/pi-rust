# POSIX Node os.homedir authority

`crates/pi-js-compat/src/node_os.rs` supplies one shared authority for frozen `node:os` consumers. HOME present, including empty, wins. HOME absent uses the effective UID's passwd entry, not USERPROFILE. Environment/passwd bytes enter through `node_fs::decode_node_path`.

| Behavior | Proof |
|---|---|
| HOME present/empty/unset; USERPROFILE ignored; malformed UTF-8 bytes | `pi-js-compat/tests/node_os.rs` Node-vs-native child-process differential (four environments) |
| SystemError message, code, info, errno, syscall, string conversion | Same test, actual Node `internal/errors.ERR_SYSTEM_ERROR` with EIO and ENOENT |
| Mutated name (including lone surrogate/empty), ordinary TypeError vs SystemError; uncaught formatting | `pi-coding-agent/tests/native_error_uncaught.rs` actual Node constructor differential |
| Missing passwd record / NSS failure | Explicit SystemError. Approved gap: safe nix 0.30.1 reads errno instead of getpwuid_r's returned errno; code/errno may differ. No fallback path. |

RED evidence: `/tmp/pi-rust-repin-lossless-interactive/native-error-red.log` (unknown panic payload); `node-os-red.log` (missing authority/class). Shared GREEN: `node-os-green.log`. This authority proof alone does not close consumer rows.

## Cleared consumer migration

| Frozen module/lines | Native consumer | Semantics |
|---|---|---|
| tui/autocomplete.ts:517–523 | pi-tui/autocomplete.rs home_dir | Shared authority for ~ and ~/ expansion |
| coding-agent/interactive-mode.ts:1460–1470 | interactive_mode.rs home_directory/format_display_path | startsWith(empty) still prepends ~ |
| components/config-selector.ts:59–70 | config_selector.rs home_dir | Shared authority, unchanged base-dir formatting |
| components/session-selector.ts:31–38 | session_selector.rs shorten_path | Empty input unchanged; empty HOME prefixes every nonempty path with ~ (source discrepancy preserved) |
| utils/paths.ts:66–70 | utils/paths.rs home_directory | Default only; explicit homeDir unchanged |
| core/package-manager.ts:216–218 | package_manager.rs home_dir | Keep HOME truthiness before os.homedir |
| config.ts:518,529 | config.rs home_directory | Shared authority for default XDG paths |
| extensions/default-footer/src/index.ts:208 | default_footer/mod.rs | Deliberately unchanged literal HOME || USERPROFILE; not os.homedir |

Consumer RED→GREEN: homedir_consumers.rs (actual frozen paths module with HOME absent) and differential_session_selector.rs (empty/absent HOME). `homedir-consumers-green.log`:5 passed,2 ignored child probes. Parent owns final broad checks. render_utils remains assigned to225.
