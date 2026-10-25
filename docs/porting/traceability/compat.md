# Compatibility entrypoint — common-pin closure

Frozen `packages/ai/src/compat.ts` at `4f9e6f8a4`; native `crates/pi-ai/src/compat/mod.rs`.

| Exports / branches | Native mapping | Current proof |
|---|---|---|
| Nine lazy factories, env/image/registry/index/legacy/image-builtins/retry wildcard forwards | Public module/root reexports | External native `compatibility_entrypoint_forwards_the_public_root` RED→GREEN; leaf rows own implementation proofs |
| getModel/getModels/getProviders | builtin catalog reads | Registry/faux differential and provider collection proofs |
| ApiProvider, stream/simple wrappers and API mismatch | Native closures return CallbackError before acquisition | Native mismatch and direct-source mismatch differential |
| register/get/list/unregister/clear/reset; replacement keeps slot | Vec registry and Arc instance identity | Direct-source registry initial/override/remove/reset ordering |
| Nine builtin APIs; non-clobber registration and builtin identity map | builtin_apis/register_built_in_api_providers | Native ordered builtin and override/reset tests; source registry differential |
| registerFauxProvider; queue/state/models/unregister | FauxCore plus source-owned registration | Native registration and direct-source registration differential |
| Explicit nonblank key, absent/blank key, scoped env, ambient auth marker | with_env_api_key and simple variant | Nine direct-source dispatch cases, including AWS_PROFILE with no network. Ambient marker suppression was reproduced RED and removed; frozen injects `<authenticated>` literally |
| Builtin model/API/registered-instance dispatch; custom and unknown API | COMPAT_MODELS versus registry | Override native tests; source dispatch/missing differential; isolated Vertex NSS failure proves synchronous native error propagation |
| stream/complete/streamSimple/completeSimple success and acquisition failure | Result stream acquisition, native result future | Native explicit/env/simple and source dispatch/mismatch/missing/NSS matrices |

Current proof:11 native tests,5 source differential tests plus isolated child helper. `/tmp/ai3-compat.log` predates the two repairs; `/tmp/ai3-compat-ambient-{red,green}.log` and `/tmp/ai3-compat-exports-{red,green}.log` cover the changes. Removed old-only deferred methods before this closure; no alternate dispatch path survives.

GAP: Faux registration random source/ID values use native randomness while preserving the eight-base36-character shape (approved randomness/ID runtime gap). Module import readiness and JavaScript object identity follow the approved runtime/owned-value rules. No new deviation. Leaf lazy-provider behavior belongs to its module row, not this dispatch row.
