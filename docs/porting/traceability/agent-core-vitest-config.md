# agent-core Vitest configuration

Frozen `4f9e6f8a4`, `packages/agent-core/vitest.config.ts:1–19` fully compared.
Only exports Vitest's default config: Node environment, test globals/30s timeout and aliases resolving AI source entrypoints. No product/runtime behavior.

**Gap:** JS-toolchain configuration is not ported. User-approved 2026-10-20 waiver explicitly revalidated for these config-only files in the Q9 continuation instructions (2026-10-22). Rust uses Cargo and the repository test runner; no TypeScript config fallback.
