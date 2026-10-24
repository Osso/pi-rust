# providers/google-vertex traceability

Source: packages/ai/src/providers/google-vertex.ts at [common pin](../source-lock.json).
Native: crates/pi-ai/src/providers/google_vertex.rs.

| Frozen behavior | Native | Proof |
|---|---|---|
| :14–35 ambient-only auth: no login callback | VertexAuth trait default | fa5260ec actual-factory descriptor RED: canLogin true vs false; full login transcript differential |
| :17–20 stored credential owns provider, empty/missing key unconfigured | VertexAuth::resolve | fa5260ec five-case actual-source credential matrix RED: empty stored key incorrectly resolves ADC |
| :22–23 nonempty ambient key before ADC | key_auth_result | provider_auth and differential_providers complete auth results |
| :25–32 ADC path nullish default, project nullish fallback, nonempty project/location | resolve_ambient_adc | provider_auth ADC/project/location and differential auth matrix |
| factory metadata/model order/API | google_vertex_provider | actual-factory descriptor; vendor protocol proof remains differential_google_api |

Old interactive API-key/ADC/service-account choices and credential.env-based ADC paths are deleted, not kept as fallbacks. Focused GREEN/workspace check: [Q15 proof](../q15-sdk-provider-proof.md). Vendor SDK request construction is outside the global-fetch provider dispatch matrix.
