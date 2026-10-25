# bun/ entry shims

Frozen common pin `4f9e6f8a4`; whole `packages/coding-agent/src/bun/cli.ts` (12lines) and imported registration shim re-read. Native: `src/bun/{cli,register_bedrock}.rs`.

| Frozen step | Native / proof |
| --- | --- |
| Set process title and suppress Node warnings | Shared CLI runtime boundaries; no extra Bun environment mutation |
| `restoreSandboxEnv()` | Gap: Bun-only sandbox environment repair, explicitly approved in AGENTS.md; Rust is not a Bun-compiled executable. |
| Import register-bedrock.ts then cli.ts | `register_bedrock()` before `run_cli_entry`; standalone bun_entry tests prove internal-runner short-circuit and exact argument/main handoff |
| `setBedrockProviderModule(bedrockProviderModule)` | Real lazy API request after the entrypoint replaces a marker module returns exact scripted Bedrock text; no live request |

The old trace's registerBunOAuthFlows step does not exist in this pin and is retired; native code already has no such call. The entry shim is ported, not marked wholesale as a Bun gap. Tests are standalone because the Bedrock override is process-global; a local mutex protects their shared override.

Focused native proof follows the commit in `/tmp/settings-models-proof-ledger.md`. Process-title/Node-warning runtime differences remain in CLI traceability, not a new duplicate contract.

GREEN atd36d36bda: four standalone native entry/registration cases, `/tmp/settings-models-bun-entry-green.log`. No production source change was needed.
