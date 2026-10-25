# api/constrained-sampling — retired at common pin

Frozen 4f9e6f8a4 has no `packages/ai/src/api/constrained-sampling.ts`. Removed the old-only native module/export, its behavioral and differential tests, and its oracle on 2026-10-25 under the re-pin removal rule. No production consumer remained. The obsolete differential reproduced ERR_MODULE_NOT_FOUND before deletion; `/tmp/ai-retire-differential_constrained_sampling-red.log`.
