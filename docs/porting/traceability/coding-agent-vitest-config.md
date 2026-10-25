# coding-agent Vitest configuration

| Frozen source | Status | Boundary |
| --- | --- | --- |
| `coding-agent/vitest.config.ts` at `4f9e6f8a4` | gap | Vitest globals/setup/Node environment/30s timeout/dependency externalization/source aliases are JS test-toolchain configuration, not runtime behavior; the2026-10-20 waiver is revalidated, with native tests owned by Cargo/run-tests.sh. |

GAP (2026-10-25 user decision): frozen `coding-agent/vitest.config.ts` contains Vitest setup, timeout and test import aliases; it is not ported to Rust, whose tests use the existing Cargo harness.
