# providers/github-copilot traceability

Source: packages/ai/src/providers/github-copilot.ts at [common pin](../source-lock.json).
Rust: crates/pi-ai/src/providers/github_copilot.rs.

| Frozen behavior | Native | Proof |
|---|---|---|
| id/name/baseUrl, token env auth, lazy OAuth name, three API entries | github_copilot_provider | differential_providers provider descriptors and dispatch |
| No subscription flag or model filter (:10–26) | default lazy flag, no filter | a12f44de actual-source RED: subscription true and picker list empty; literal port removes both |
| Full catalog independent of credential.availableModelIds | default unfiltered models | provider_auth credential matrix; source descriptor pickerModels comparison |

Historical pre-repin filter/subscription claims are retired, not preserved as compatibility paths. Current focused GREEN/checkpoint results remain to record.
