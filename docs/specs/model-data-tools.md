# Model-data maintenance APIs — retired

Frozen `4f9e6f8a4` removed `ai/scripts/model-data.ts` and `check-model-data.ts`. Their native module, checker, lib export and product fixtures/oracle must not remain. No replacement API/CLI or grouped/schema3 manifest validation is required.

Keep the test-only [chat catalog provenance](chat-catalogs.md) and generic oracle lifetime assertions. [Deletion and named test retirement](../porting/traceability/model-data-tools.md).
