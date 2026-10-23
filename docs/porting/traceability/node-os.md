# POSIX Node os.homedir authority

`crates/pi-js-compat/src/node_os.rs` supplies one shared authority for frozen `node:os` consumers. HOME present, including empty, wins. HOME absent uses the effective UID's passwd entry, not USERPROFILE. Environment/passwd bytes enter through `node_fs::decode_node_path`.

| Behavior | Proof |
|---|---|
| HOME present/empty/unset; USERPROFILE ignored; malformed UTF-8 bytes | `pi-js-compat/tests/node_os.rs` Node-vs-native child-process differential (four environments) |
| SystemError message, code, info, errno, syscall, string conversion | Same test, actual Node `internal/errors.ERR_SYSTEM_ERROR` with EIO and ENOENT |
| Mutated name (including lone surrogate/empty), ordinary TypeError vs SystemError; uncaught formatting | `pi-coding-agent/tests/native_error_uncaught.rs` actual Node constructor differential |
| Missing passwd record / NSS failure | Explicit SystemError. Approved gap: safe nix 0.30.1 reads errno instead of getpwuid_r's returned errno; code/errno may differ. No fallback path. |

RED evidence: `/tmp/pi-rust-repin-lossless-interactive/native-error-red.log` (unknown panic payload); `node-os-red.log` (missing authority/class). Shared GREEN: `node-os-green.log`. This authority proof alone does not close consumer rows.
