# Lossless runtime/mailbox traceability

Contract: [lossless runtime/mailbox](../../specs/lossless-runtime-mailbox.md).

| Source/API boundary | Behavioral target | Proof |
| --- | --- | --- |
| Canonical mailbox public body and claim | `repin_control_raw_json::canonical_mailbox_source_surrogate_body_survives_native_projection_and_claim` | Historical library-blocked attempt superseded by runtime PASS at `65e23c966`; exact source/native public body and claim preserved, `/tmp/storage2-public-payload-final.log` |
| Enqueue single authority, idempotent retry, DB restore, file refs, replacement collision | `lossless_runtime_mailbox::enqueue_retry_restore_retains_raw_payload_and_rejects_replacement_collision` | Added; library integration pending |
| Store public mailbox serialization / UTF-16 IDs and arbitrary unknown keys | `lossless_runtime_mailbox::store_message_serialization_preserves_raw_addresses_file_refs_and_unknown_keys` | Added; library integration pending |
| Supervisor request DB payload and response serialization | `lossless_runtime_mailbox::supervisor_request_and_response_preserve_raw_content` | Added; library integration pending |
| Detached control response/cancel delivery | `lossless_runtime_mailbox::detached_control_delivery_keeps_result_error_and_cancel_reason` | Added; library integration pending |
| Recipient receipt encoding | `runtime_mailbox_acceptance` now serializes refs directly with `js_json::to_value` | Existing acceptance targets await parent integration |
| Completion summary notification | `lossless_runtime_mailbox::terminal_notifications_keep_content_units_and_duration` | Added; library integration pending |
| Raw name normalization and SQL binding | `lossless_runtime_mailbox::session_name_normalization_retains_units_until_sql_text_binding` | RED attempt library-blocked before test compilation |

Original JSON CAS is retained. Read-only scalar schema validation is not used as a persisted write or public content authority. SQLite keys are encoded only at concrete storage/helper boundaries.

Proof artifacts and exact public signatures: `/tmp/pi-rust-repin-lossless-runtime/REPORT.md`. Broad suites, workspace acceptance, deployment and full parity are outside this bounded lane. Q16 `c0becd16` is not merged; parent reconciles its future conflicts and the host-relay helper delta.
