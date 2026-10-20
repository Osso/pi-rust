# Example: custom-header.ts

Source: `packages/coding-agent/examples/extensions/custom-header.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`; [source lock](../source-lock.json). Contract: [feature spec](../../specs/example-custom-header.md). Coverage remains **partial** until parent final verification.

## Native embedding

Include `crates/pi-coding-agent/examples/extensions/custom_header.rs` as a module in an embedding application. Its public `custom_header_extension(&ExtensionApi) -> Result<(), HandlerError>` registers the example. Wrap that function in an `ExtensionFactory`, register it in an explicitly chosen `ExtensionRegistry`, and load through the existing native loader. No `main`, library export, default autoload, or bundled registry entry is added. The integration test demonstrates this composition with `#[path = "../examples/extensions/custom_header.rs"]`.

## Branch and symbol proof

All rows map to `pinned_custom_header_composition_matches_all_modes_and_colors` in `crates/pi-coding-agent/tests/example_custom_header.rs` and `oracle/example-custom-header-oracle.ts`.

| Pinned symbol/branch | Native mapping | Observable proof |
| --- | --- | --- |
| default export registration | `custom_header_extension` | Real native API factory load; exactly one session handler and command, exact command name/description; no UI effects during load |
| `session_start`, mode `tui` | `on_session_start`, `ExtensionMode::Tui` | Exact install/factory/render/invalidate/clear/notify sequence |
| non-TUI `session_start` | Print, RPC, JSON modes | Exact clear/notify-only command sequence; session callback causes no UI call |
| `getPiMascot` | private `pi_mascot` | Entire nine-line returned header vector, including all eight mascot lines; no filtering, normalization, or ANSI removal |
| white block, dim pupil, accent roof/legs, muted subtitle | Factory `Theme`, `ThemeColor` roles | Two distinct RGB fixtures; exact ANSI bytes, glyphs, spacing and foreground resets |
| `render(_width)` | `CustomHeader::render` | Widths 0, 1, 16, 80, 200 produce exact source output |
| subtitle `VERSION` | `config::version()` | Private package fixture version `8.7.6-header-fixture`; controlled TS public VERSION export matches actual Rust package API |
| `invalidate()` | empty `CustomHeader::invalidate` | Re-render after invalidation exactly matches source; stateless component contract, not a missing implementation |
| `/builtin-header` | `register_command` | Arbitrary String args ignored; clearing precedes exact info notice in every tested mode |

The oracle imports actual source bytes without copying or rewriting the example. A Node resolution hook replaces only the public package import with the fixture VERSION export; UI and theme are controlled observable boundaries. The existing `TsOracle` verifies source HEAD and uses `parent-watchdog.mjs`. Native tests invoke the factory with real `Theme`, `ExtensionUiFactoryContext`, and runner-backed context, recording exact rendered output and UI calls; no live CLI, provider, DB, or bundled extension startup.

## Proof ledger

External ledger: `/tmp/pi-rust-example-header-proof`.

| Command | Scope | Result |
| --- | --- | --- |
| `cargo test --offline -p pi-coding-agent --test example_custom_header -- --nocapture` | Base `21d2eed` plus test/oracle and inert registration scaffold | RED: `session_start` count 0 instead of 1 |
| Same targeted command | Literal implementation, first oracle run | Harness failure: TSX CJS rejects top-level await; oracle entrypoint corrected, no source filtering |
| Same targeted command | Literal implementation and corrected oracle | GREEN: 1 test; all 8 mode/theme combinations, 5 widths and post-invalidation render |

No semantic discrepancy or new dependency identified. Native extension adaptation and String-only argument boundary use existing approvals. No broad/check/clippy/readability/final gate claimed; parent owns final verification.
