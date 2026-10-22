# Lossless runtime/mailbox traceability

Contract: [lossless runtime/mailbox](../../specs/lossless-runtime-mailbox.md).

| Source/API boundary | Behavioral target | Proof |
| --- | --- | --- |
| Canonical mailbox public body and claim | `repin_control_raw_json::canonical_mailbox_source_surrogate_body_survives_native_projection_and_claim` | RED attempt blocked by pre-existing coding-agent library errors; no runtime result |
| Enqueue single authority, idempotent retry, DB restore, file refs, replacement collision | `lossless_runtime_mailbox::enqueue_retry_restore_retains_raw_payload_and_rejects_replacement_collision` | Added; library integration pending |
| Store public mailbox serialization / UTF-16 IDs and arbitrary unknown keys | `lossless_runtime_mailbox::store_message_serialization_preserves_raw_addresses_file_refs_and_unknown_keys` | Added; library integration pending |
| Supervisor request DB payload and response serialization | `lossless_runtime_mailbox::supervisor_request_and_response_preserve_raw_content` | Added; library integration pending |
| Recipient receipt encoding | `runtime_mailbox_acceptance` now serializes refs directly with `js_json::to_value` | Existing acceptance targets await parent integration |
| Completion summary notification | UTF-16 trim/concatenation; no scalar `as_deref` | Runtime test pending parent integration |

Original JSON CAS is retained. Read-only scalar schema validation is not used as a persisted write or public content authority. SQLite keys are encoded only at concrete storage/helper boundaries.

Proof artifacts and exact public signatures: `/tmp/pi-rust-repin-lossless-runtime/REPORT.md`. Broad suites, workspace acceptance, deployment and full parity are outside this bounded lane. Q16 `c0becd16` is not merged; parent reconciles its future conflicts and the host-relay helper delta.
