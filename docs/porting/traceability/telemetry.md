# Removed old telemetry package

The common pin `4f9e6f8a4` contains no `packages/telemetry` and no
`InMemoryTelemetryContext`/`defineTelemetrySchema` implementation anywhere in
`packages/`. RED: `differential_telemetry` cannot import the absent package.
The unused `pi-telemetry` crate, its tests and its oracle were removed on
2026-10-25 under the explicit baseline-triage re-pin rule. No remaining crate
depended on it. This is not the unrelated install-telemetry flag helper at
`coding-agent/src/core/telemetry.ts`; existing statistics behavior is unchanged.
