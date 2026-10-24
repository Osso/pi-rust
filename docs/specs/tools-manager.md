# Tools manager

Discover fd/ripgrep from PATH, matching frozen `coding-agent/src/utils/tools-manager.ts` at immutable4f9e6f8a4. [Module map](../porting/traceability/tools-manager.md).

## What it must do

- [x] Try `fd` before `fdfind`; use `rg` for ripgrep. Run `--version`, suppress its stdout/stderr, and treat nonzero exit as available when spawning succeeds.
- [x] Ignore old managed binaries and offline settings. Never configure/read an agent directory, fetch HTTP, install or extract a release, or print old download/offline messages.
- [x] Return the command name when found; otherwise return no path. Silent lookup prints nothing; non-silent failure prints the frozen missing-tool/package-manager hint.
- [x] Preserve Android's fd/ripgrep package names and trailing punctuation under the frozen platform branch.

## How it works

- [Symbol/branch mapping and proof](../porting/traceability/tools-manager.md).

## Implementation inventory

- `crates/pi-coding-agent/src/utils/tools_manager.rs`: closed native tool domain and PATH lookup/ensure functions.
- `core/tools/{find,grep}.rs`: silent lookup consumers.
- `modes/interactive/interactive_mode.rs`: concurrent non-silent startup lookup; HTTP dispatcher remains independent.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/tools_manager.rs`: actual frozen lookup/ensure differential, real executable probes and child-process console output. Replaces all retired asset/download/extract/install tests.
- Native Android hint unit test in `utils/tools_manager.rs`.

## Known gaps (current cycle)

- [ ] Final own-source clippy recheck after routing console output through the existing console authority.

## Out of scope

Old managed-bin/download/install behavior is removed, not a compatibility path. Invalid JavaScript tool-name values are excluded by the approved typed native API boundary. Node spawnSync's implicit default maxBuffer is an approved runtime gap: a spawnable --version command printing over1MiB stays available natively; no implicit Node buffer-limit emulation.
