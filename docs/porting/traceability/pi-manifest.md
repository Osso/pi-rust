# Shared native pi manifest reader

Frozen `4f9e6f8a4` keeps `readPiManifest` private in `core/extensions/loader.ts`
and `readPiManifestFile` private in `core/package-manager.ts`; there is no
standalone `core/pi-manifest.ts` export. The native shared implementation stays
because both corresponding consumers use it.

`differential_core_leaves::pi_manifest_discovery_matches_frozen_public_consumer`
compares native discovery paths with frozen `discoverAndLoadExtensions` paths
using five real package directories: explicit ordered entries, empty pi object,
empty extension list, missing/duplicate entries, and no pi field. All factories
are loaded by frozen unchanged; the oracle rejects any load error.

| Branch | Proof |
|---|---|
| Valid manifest discovery | Exact public-consumer differential above |
| Native malformed-input projection | `pi_manifest.rs` unit tests; not frozen parity |
| gap | Wrong-typed `pi` manifest fields follow the typed native projection, not frozen's raw `pkg.pi` handling (approved 2026-10-25). |
