# providers/radius removal

Frozen `4f9e6f8a4` has no `packages/ai/src/providers/radius.ts`; the former
`532f7d12` port is removed, not a parity claim or approved fallback.

Removed module/export, public `RadiusProviderOptions`, `RadiusProvider`,
`radius_provider`, `radius_provider_with_fetch`, private refresh/publication
state, and factory tests in `provider_auth.rs`. Coding-agent factory synthesis
from Radius config is removed; ordinary builtin reset remains. The generic
OAuth subscription test retains Anthropic/OpenRouter, not nonexistent Radius.

Evidence, retained APIs and proof boundary:
[factory removal](providers.md#removed-old-only-factories).
Radius auth/config and pi-messages modules remain separate old-pin work;
this factory removal neither revalidates nor removes them.
