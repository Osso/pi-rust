# Removed experimental CLI: experimental-cli

Frozen `4f9e6f8a4` contains no `coding-agent/src/cli/experimental/`
modules or equivalent command composition/auth/transport exports. The isolated
Rust experimental CLI and its tests were removed on 2026-10-25 under the
baseline-triage re-pin rule. Core-leaf REDs confirmed missing frozen imports.
This is unrelated to `core/experimental.ts`, which remains in the source pin.
