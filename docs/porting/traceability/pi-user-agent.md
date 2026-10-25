# coding-agent pi-user-agent traceability

Pinned source: `packages/coding-agent/src/utils/pi-user-agent.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `26005` executes the real pinned module and compares its runtime platform, architecture, runtime token, and five formatted user agents. Focused tests: `crates/pi-coding-agent/tests/pi_user_agent.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `pi-user-agent.ts:1-4` | `pi/{version} ({platform}; {runtime}; {arch})` | `utils::pi_user_agent::format_pi_user_agent`; focused `formats_the_user_agent_expected_by_pi_dev`, differential `26005` |
| `pi-user-agent.ts:2` | Runtime token is `bun/{version}` or `node/{version}` | `get_pi_user_agent` reports `rust`; see discrepancy |
| `pi-user-agent.ts:3` | Platform uses `process.platform`; architecture uses `process.arch` | `node_platform_name` / `node_arch_name` map Rust `consts::OS`/`ARCH`; focused mapping tests and differential cross-check against the live Node values |

## Runtime gap

- The native binary truthfully reports `rust` instead of Node/Bun runtime metadata; name, version, platform, architecture, separators and order must otherwise match exactly (parent classification 2026-10-25).

## Platform mapping

- Platform/architecture names are mapped to Node spellings (`macos`→`darwin`, `windows`→`win32`, `x86_64`→`x64`, `aarch64`→`arm64`, `x86`→`ia32`, `powerpc64`→`ppc64`, `loongarch64`→`loong64`, `mips64`→`mips64el`).

## Proof inventory

- Focused cases: formatting, platform mapping, architecture mapping, runtime shape.
- Differential request: `26005`, 5 versions plus live platform/arch/runtime cross-check.

## Q18 native version input (2026-10-22)

User-agent format/get functions preserve native version units; platform/runtime/
arch remain scalar metadata tags. Remote catalog/version HTTP construction encodes
the resulting user-agent at the header boundary. Interactive direct callers pass
native version input. Raw-version fixture written but unexecuted while coding
compilation remains blocked. No runtime PASS.
