//! The frozen combine helper's synchronous reason, cleanup and returned-signal lifetimes.
use crate::shared_abort_reason as abort_reason;
use crate::shared_oracle as oracle;

use pi_ai::utils::abort_signals::combine_abort_signals;
use pi_js_compat::abort::{AbortController, AbortReason, AbortSignal};
use pi_js_compat::js_json::{self, JsString, JsValue};
use pi_js_compat::node_error::NativeNodeError;

fn decode(value: &JsValue) -> Option<AbortReason> {
    let kind = value["kind"].as_string().unwrap();
    if kind == "default" {
        return None;
    }
    if kind == "value" {
        return Some(AbortReason::Value(value["value"].clone()));
    }
    if kind == "number" {
        let text = value["value"].as_string().unwrap().to_utf8().unwrap();
        let number = match text.as_str() {
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "NaN" => f64::NAN,
            _ => text.parse().unwrap(),
        };
        return Some(AbortReason::Value(JsValue::Number(number)));
    }
    let name = value["name"].as_string().cloned().unwrap_or_else(|| "Error".into());
    let message = value["message"].as_string().cloned().unwrap_or_default();
    let mut error = NativeNodeError::new(name.clone(), message);
    if kind == "dom" {
        let code = if name == "AbortError" {
            20.0
        } else if name == "TimeoutError" {
            23.0
        } else {
            0.0
        };
        error = error.with_property("code", JsValue::Number(code));
    }
    if let Some(properties) = value["properties"].as_object() {
        for (name, value) in properties {
            error = error.with_property(name, value.clone());
        }
    }
    Some(if kind == "dom" {
        AbortReason::DomException(error)
    } else {
        AbortReason::Error(error)
    })
}

fn snapshot(signal: Option<&AbortSignal>) -> JsValue {
    signal.map_or_else(
        || pi_js_compat::js_json!({"kind":"no-signal"}),
        |signal| pi_js_compat::js_json!({"aborted":signal.aborted(),"reason":abort_reason::describe(signal.reason())}),
    )
}

fn observe(input: &JsValue) -> JsValue {
    let controllers: Vec<_> = input["present"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, present)| {
            if !present.as_bool().unwrap() {
                return None;
            }
            let controller = AbortController::new();
            let initial = &input["initial"].as_array().unwrap()[index];
            if initial != &JsValue::Null {
                controller.abort(decode(initial));
            }
            Some(controller)
        })
        .collect();
    let sources: Vec<_> = controllers
        .iter()
        .map(|controller| controller.as_ref().map(AbortController::signal))
        .collect();
    let mut combined = Some(combine_abort_signals(&sources));
    let signal = combined.as_ref().unwrap().signal().cloned();
    let same_sources: Vec<_> = sources
        .iter()
        .map(|source| {
            source
                .as_ref()
                .zip(signal.as_ref())
                .is_some_and(|(source, result)| source == result)
        })
        .collect();
    let mut snapshots = vec![snapshot(signal.as_ref())];
    if input["dropWrapper"].as_bool() == Some(true) {
        drop(combined.take());
    }
    for action in input["actions"].as_array().unwrap() {
        if action["kind"].as_string().unwrap() == "cleanup" {
            combined.as_ref().unwrap().cleanup();
        } else {
            let index = action["index"].as_f64().unwrap() as usize;
            controllers[index].as_ref().unwrap().abort(decode(&action["reason"]));
        }
        snapshots.push(snapshot(signal.as_ref()));
    }
    pi_js_compat::js_json!({"sameSources":same_sources,"snapshots":snapshots})
}

fn compare(oracle: &mut oracle::TsOracle, case: JsValue) {
    let expected = oracle.request(&case);
    assert_eq!(
        expected["ok"],
        JsValue::Bool(true),
        "{}",
        js_json::stringify_lossless(&expected)
    );
    assert_eq!(
        observe(&case),
        expected["output"],
        "case {}",
        js_json::stringify_lossless(&case)
    );
}
fn oracle() -> oracle::TsOracle {
    oracle::TsOracle::spawn("abort-combine-oracle.ts", "packages/ai/src/utils/abort-signals.ts")
}

#[tokio::test]
async fn zero_single_input_identity_and_noop_cleanup_match_frozen_module() {
    let mut oracle = oracle();
    for present in [vec![], vec![false], vec![false, false]] {
        let initial = vec![JsValue::Null; present.len()];
        compare(
            &mut oracle,
            pi_js_compat::js_json!({"present":present,"initial":initial,"actions":[{"kind":"cleanup"},{"kind":"cleanup"}]}),
        );
    }
    compare(
        &mut oracle,
        pi_js_compat::js_json!({"present":[false,true,false],"initial":[null,null,null],"actions":[{"kind":"cleanup"},{"kind":"abort","index":1,"reason":{"kind":"value","value":null}},{"kind":"cleanup"}]}),
    );
}

#[tokio::test]
async fn multi_input_first_reason_is_forwarded_synchronously_without_repair() {
    let mut oracle = oracle();
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let descriptors = vec![
        pi_js_compat::js_json!({"kind":"default"}),
        pi_js_compat::js_json!({"kind":"value","value":null}),
        pi_js_compat::js_json!({"kind":"value","value":false}),
        pi_js_compat::js_json!({"kind":"value","value":raw.clone()}),
        pi_js_compat::js_json!({"kind":"value","value":""}),
        pi_js_compat::js_json!({"kind":"value","value":[null,false,raw.clone()]}),
        pi_js_compat::js_json!({"kind":"number","value":"-0"}),
        pi_js_compat::js_json!({"kind":"number","value":"NaN"}),
        pi_js_compat::js_json!({"kind":"number","value":"Infinity"}),
        pi_js_compat::js_json!({"kind":"value","value":{"name":"TimeoutError","message":raw.clone(),"stack":"plain data, not an Error"}}),
        pi_js_compat::js_json!({"kind":"error","name":"CustomError","message":raw,"properties":{"code":"E_CUSTOM","cause":{"nested":true}}}),
        pi_js_compat::js_json!({"kind":"dom","name":"TimeoutError","message":"custom deadline"}),
    ];
    for descriptor in descriptors {
        compare(
            &mut oracle,
            pi_js_compat::js_json!({"present":[true,false,true,true],"initial":[null,null,null,null],"actions":[{"kind":"abort","index":2,"reason":descriptor.clone()},{"kind":"abort","index":0,"reason":{"kind":"value","value":"ignored"}},{"kind":"cleanup"}]}),
        );
        compare(
            &mut oracle,
            pi_js_compat::js_json!({"present":[true,true],"initial":[descriptor.clone(),{"kind":"value","value":"later preabort"}],"actions":[]}),
        );
        compare(
            &mut oracle,
            pi_js_compat::js_json!({"present":[true,false,true,true],"initial":[null,null,descriptor,null],"actions":[{"kind":"abort","index":0,"reason":{"kind":"value","value":"ignored after preabort"}},{"kind":"cleanup"}]}),
        );
    }
}

#[tokio::test]
async fn cleanup_stops_forwarding_and_discarding_wrapper_does_not() {
    let mut oracle = oracle();
    compare(
        &mut oracle,
        pi_js_compat::js_json!({"present":[true,true],"initial":[null,null],"actions":[{"kind":"cleanup"},{"kind":"cleanup"},{"kind":"abort","index":0,"reason":{"kind":"value","value":"detached"}},{"kind":"abort","index":1,"reason":{"kind":"value","value":"also detached"}}]}),
    );
    compare(
        &mut oracle,
        pi_js_compat::js_json!({"present":[true,true],"initial":[null,null],"dropWrapper":true,"actions":[{"kind":"abort","index":1,"reason":{"kind":"error","name":"KeptError","message":"retained signal"}}]}),
    );
}

#[test]
fn combining_signals_is_synchronous_without_an_async_runtime() {
    compare(
        &mut oracle(),
        pi_js_compat::js_json!({"present":[true,true],"initial":[null,null],"actions":[{"kind":"abort","index":0,"reason":{"kind":"value","value":"no runtime"}}]}),
    );
}
