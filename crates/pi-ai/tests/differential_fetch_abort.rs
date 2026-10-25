use crate::shared_abort_reason as abort_reason;
use crate::shared_oracle as oracle;

use pi_ai::http::default_fetch;
use pi_ai::request::{CallbackError, HttpRequest};
use pi_js_compat::abort::{AbortController, AbortReason};
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::NativeNodeError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn decode(input: &JsValue) -> Option<AbortReason> {
    match input["kind"].as_string().unwrap().to_utf8().unwrap().as_str() {
        "default" => None,
        "number" => Some(AbortReason::Value(JsValue::Number(
            match input["value"].as_string().unwrap().to_utf8().unwrap().as_str() {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                "-0" => -0.0,
                _ => panic!("unknown number descriptor"),
            },
        ))),
        "error" => Some(AbortReason::Error(
            NativeNodeError::new(
                input["name"].as_string().cloned().unwrap_or_else(|| "Error".into()),
                input["message"].as_string().unwrap().clone(),
            )
            .with_property("detail", input["properties"]["detail"].clone()),
        )),
        "dom" => Some(AbortReason::DomException(
            NativeNodeError::new(
                input["name"].as_string().unwrap().clone(),
                input["message"].as_string().unwrap().clone(),
            )
            .with_property("code", JsValue::Number(23.0)),
        )),
        _ => Some(AbortReason::Value(input["value"].clone())),
    }
}

async fn reject(phase: &str, reason: Option<AbortReason>) -> CallbackError {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let controller = AbortController::new();
    let server_controller = controller.clone();
    let phase_owned = phase.to_owned();
    let server_reason = reason.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let _request_len = socket.read(&mut request).await.unwrap();
        if phase_owned == "body" {
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await
                .unwrap();
        } else {
            server_controller.abort(server_reason);
        }
        std::future::pending::<()>().await;
    });
    if phase == "pre" {
        controller.abort(reason.clone());
    }
    let request = HttpRequest {
        method: "GET".into(),
        url,
        headers: Vec::new(),
        body: None,
    };
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(4), async {
        match default_fetch().fetch(request, Some(controller.signal())).await {
            Err(error) => error,
            Ok(mut response) => {
                assert_eq!(phase, "body");
                controller.abort(reason);
                response.body.next_chunk().await.unwrap().unwrap_err()
            }
        }
    })
    .await;
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    outcome.expect("bounded abort rejection")
}

#[tokio::test]
async fn fetch_abort_rejections_match_native_node_at_send_and_body_boundaries() {
    let mut oracle = oracle::TsOracle::spawn("fetch-abort-oracle.ts", "packages/ai/src/types.ts");
    let cases = pi_js_compat::js_json::parse_lossless(
        r#"[
        {"kind":"default"},
        {"kind":"value","value":null},
        {"kind":"value","value":false},
        {"kind":"value","value":0},
        {"kind":"value","value":""},
        {"kind":"value","value":"stop\ud800"},
        {"kind":"value","value":{"name":"PretendError","message":"raw\ud800"}},
        {"kind":"value","value":{"toString":false,"native":"raw\ud800"}},
        {"kind":"value","value":["native\ud800",null,false]},
        {"kind":"number","value":"-0"},
        {"kind":"number","value":"NaN"},
        {"kind":"number","value":"Infinity"},
        {"kind":"number","value":"-Infinity"},
        {"kind":"value","value":42},
        {"kind":"error","name":"OAuthFailure","message":"native\ud800","properties":{"detail":{"text":"detail\udc00"}}},
        {"kind":"dom","name":"TimeoutError","message":"The operation was aborted due to timeout"}
    ]"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for phase in ["pre", "live", "body"] {
        for input in cases.as_array().unwrap() {
            let mut request = input.clone();
            request
                .as_object_mut()
                .unwrap()
                .insert("phase".into(), JsValue::String(phase.into()));
            request
                .as_object_mut()
                .unwrap()
                .insert("id".into(), JsValue::String("fetch-abort".into()));
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
            let error = reject(phase, decode(input)).await;
            let actual = abort_reason::describe(Some(error.thrown_value()));
            let expected_fields = &expected["output"]["rejection"];
            let input_text = pi_js_compat::js_json::stringify_lossless(input);
            let expected_text = pi_js_compat::js_json::stringify_lossless(&expected);
            let actual_text = pi_js_compat::js_json::stringify_lossless(&actual);
            eprintln!("phase={phase} input={input_text} source={expected_text}");
            if &actual != expected_fields {
                failures.push(format!(
                    "phase={phase} input={input_text} actual={actual_text} source={expected_text}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
