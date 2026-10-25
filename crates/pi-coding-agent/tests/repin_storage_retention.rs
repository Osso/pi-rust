use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_control_db::{MAILBOX_MESSAGE_RETENTION_MS, is_mailbox_message_expired};
use pi_js_compat::js_json::{JsObject, JsString, JsValue};

#[test]
fn mailbox_retention_matches_frozen_unknown_ingress_and_numeric_boundaries() {
    let mut ts = oracle::TsOracle::spawn("storage-retention-oracle.mts", "packages/coding-agent/src/core");
    let strings = [
        "1970-01-01T00:00:00.000Z",
        "1969-12-31T23:59:59.999Z",
        "1970-01-02T00:00:00.000Z",
        "",
        "not-a-date",
        "2026-02-30",
        "+275760-09-13T00:00:00.000Z",
        "+275760-09-13T00:00:00.001Z",
    ];
    let mut values: Vec<Option<JsValue>> = strings
        .into_iter()
        .map(|value| Some(JsValue::String(value.into())))
        .collect();
    values.extend([
        None,
        Some(JsValue::Null),
        Some(JsValue::Bool(false)),
        Some(JsValue::Number(0.0)),
        Some(JsValue::Array(vec![])),
        Some(JsValue::Object(JsObject::new())),
        Some(JsValue::String(JsString::from_utf16(vec![0xd800]))),
    ]);
    let now_cases = [
        (0.0, None),
        (86_399_999.0, None),
        (86_400_000.0, None),
        (86_400_001.0, None),
        (-86_400_000.0, None),
        (f64::NAN, Some("nan")),
        (f64::INFINITY, Some("infinity")),
        (f64::NEG_INFINITY, Some("negativeInfinity")),
    ];
    for value in values {
        for (now, kind) in now_cases {
            let mut request = JsObject::from([("id".into(), JsValue::Number(1.0))]);
            if let Some(value) = &value {
                request.insert("createdAt".into(), value.clone());
            }
            if let Some(kind) = kind {
                request.insert("nowKind".into(), JsValue::String(kind.into()));
            } else {
                request.insert("now".into(), JsValue::Number(now));
            }
            let expected = ts.request(&JsValue::Object(request));
            assert_eq!(expected["ok"], JsValue::Bool(true));
            assert_eq!(
                expected["output"]["retention"],
                JsValue::Number(MAILBOX_MESSAGE_RETENTION_MS as f64)
            );
            // Frozen's first conversion is typeof createdAt === "string".
            let actual = is_mailbox_message_expired(value.as_ref().and_then(JsValue::as_string), now);
            assert_eq!(
                JsValue::Bool(actual),
                expected["output"]["expired"],
                "{value:?} now={now}"
            );
        }
    }
}
