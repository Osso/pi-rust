#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::modes::rpc::{RpcClient, RpcClientError, RpcClientOptions, RpcCommandBody};
use pi_js_compat::js_json::{self, JsValue};
use std::time::Duration;

const CHILD: &str = include_str!("../../../oracle/rpc-client-child.mjs");
const SOURCE: &str = "packages/coding-agent/src/modes/rpc/rpc-client.ts";

async fn native(request: &JsValue) -> JsValue {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("child.mjs");
    std::fs::write(&path, CHILD).unwrap();
    let mut options = RpcClientOptions {
        cli_path: Some(path),
        ..Default::default()
    };
    options.env.insert(
        "PI_RPC_CLIENT_REPLY".into(),
        js_json::stringify_lossless(request.get("reply").unwrap()),
    );
    options.env.insert(
        "PI_RPC_EXTRA_EVENTS".into(),
        js_json::stringify_lossless(request.get("events").unwrap_or(&JsValue::Array(Vec::new()))),
    );
    let client = RpcClient::new(options);
    let removal = std::sync::Arc::new(std::sync::Mutex::new(None));
    if request.get("listenerRemovesItself").and_then(JsValue::as_bool) == Some(true) {
        let slot = removal.clone();
        let subscription = client.on_event(move |_| {
            drop(slot.lock().unwrap().take());
            Ok(())
        });
        *removal.lock().unwrap() = Some(subscription);
    }
    let _fault = if request.get("listenerFault").and_then(JsValue::as_bool) == Some(true) {
        Some(client.on_event(|event| {
            if event.get("willRetry").and_then(JsValue::as_bool) == Some(true) {
                return Err(pi_ai::request::CallbackError::new("fixture callback"));
            }
            Ok(())
        }))
    } else {
        None
    };
    client.start().await.unwrap();
    let result = if request.get("operation").and_then(JsValue::as_string) == Some(&"timeout".into()) {
        let delay = match request.get("delay").unwrap() {
            JsValue::Number(value) => *value,
            JsValue::String(value) if value == "NaN" => f64::NAN,
            JsValue::String(value) if value == "Infinity" => f64::INFINITY,
            JsValue::String(value) if value == "-Infinity" => f64::NEG_INFINITY,
            _ => panic!("unknown timer fixture"),
        };
        if request.get("idleTimeout").and_then(JsValue::as_bool) == Some(true) {
            tokio::time::timeout(Duration::from_secs(2), client.wait_for_idle(delay))
                .await
                .expect("normalized idle timeout must settle")
                .map(|()| pi_js_compat::js_json!({"ok":true}))
        } else {
            tokio::time::timeout(Duration::from_secs(2), client.collect_events(delay))
                .await
                .expect("normalized collection timeout must settle")
                .map(|value| pi_js_compat::js_json!({"ok":true,"value":value}))
        }
    } else if request.get("operation").and_then(JsValue::as_string) == Some(&"collector_skip".into()) {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<JsValue>::new()));
        let captured = seen.clone();
        let (events, (sent, _watcher)) = tokio::join!(client.collect_events(2000.0), async {
            let watcher = client.on_event(move |event| {
                captured.lock().unwrap().push(event.clone());
                Ok(())
            });
            (client.prompt("input", None).await, watcher)
        });
        sent.unwrap();
        events.map(
            |events| pi_js_compat::js_json!({"ok":true,"value":{"events":events,"seen":seen.lock().unwrap().clone()}}),
        )
    } else if request.get("operation").and_then(JsValue::as_string) == Some(&"typed".into()) {
        match request
            .get("method")
            .and_then(JsValue::as_string)
            .and_then(|s| s.to_utf8().ok())
            .as_deref()
        {
            Some("model") => client
                .set_model("provider", "id")
                .await
                .map(|value| pi_js_compat::js_json!({"ok":true,"value":value})),
            Some("cycle") => client
                .cycle_model()
                .await
                .map(|value| pi_js_compat::js_json!({"ok":true,"value":value})),
            Some("models") => client
                .get_available_models()
                .await
                .map(|value| pi_js_compat::js_json!({"ok":true,"value":value})),
            Some("new") => client
                .new_session(None)
                .await
                .map(|value| pi_js_compat::js_json!({"ok":true,"value":value})),
            Some("text") => client
                .get_last_assistant_text()
                .await
                .map(|value| pi_js_compat::js_json!({"ok":true,"value":value})),
            _ => panic!("unknown fixture method"),
        }
    } else if request.get("operation").and_then(JsValue::as_string) == Some(&"send".into()) {
        client
            .send(RpcCommandBody::GetState)
            .await
            .map(|value| pi_js_compat::js_json!({"ok":true,"value":value}))
    } else {
        let (events, sent) = tokio::join!(client.collect_events(2000.0), client.prompt("input", None));
        sent.unwrap();
        events.map(|value| pi_js_compat::js_json!({"ok":true,"value":value}))
    };
    client.stop().await.unwrap();
    match result {
        Ok(value) => value,
        Err(RpcClientError::CommandFailed { message }) => {
            pi_js_compat::js_json!({"ok":false,"error":{"name":"Error","message":message}})
        }
        Err(RpcClientError::TypeError { message }) => {
            pi_js_compat::js_json!({"ok":false,"error":{"name":"TypeError","message":message}})
        }
        Err(error) => pi_js_compat::js_json!({"ok":false,"error":{"name":"Error","message":error.to_string()}}),
    }
}

#[tokio::test]
async fn rpc_client_number_timeouts_use_the_shared_node_normalization() {
    // Frozen :470/:490 pass the numeric delay directly to setTimeout.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    for delay in [
        JsValue::Number(0.0),
        JsValue::Number(-1.0),
        JsValue::Number(0.5),
        JsValue::Number(2147483648.0),
        JsValue::String("NaN".into()),
        JsValue::String("Infinity".into()),
        JsValue::String("-Infinity".into()),
    ] {
        for idle in [false, true] {
            let request = pi_js_compat::js_json!({"operation":"timeout","delay":delay.clone(),"idleTimeout":idle,"reply":{"type":"response","success":true}});
            let expected = source.request(&request);
            assert_eq!(native(&request).await, expected);
        }
    }
}

#[tokio::test]
async fn rpc_client_collection_unsubscribes_during_the_terminal_notification() {
    // Frozen :498-500 removes the current listener inside the live dispatch loop.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    let request = pi_js_compat::js_json!({"operation":"collector_skip","reply":{"type":"response","success":true,"command":"prompt"}});
    let expected = source.request(&request);
    assert_eq!(native(&request).await, expected);
}

#[tokio::test]
async fn rpc_client_listener_throw_and_live_unsubscribe_match_the_caught_source_loop() {
    // Frozen :539-545 catches callback failures and iterates the live listener array.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    for (fault, removes) in [(true, false), (false, true)] {
        let request = pi_js_compat::js_json!({"operation":"collect","events":[{"type":"skip"}],"listenerFault":fault,"listenerRemovesItself":removes,"reply":{"type":"response","success":true,"command":"prompt"}});
        let expected = source.request(&request);
        assert_eq!(native(&request).await, expected, "fault={fault}, removes={removes}");
    }
}

#[tokio::test]
async fn rpc_client_line_dispatch_ignores_null_but_delivers_other_json_values() {
    // Frozen :523-545 catches null.type; every other primitive boxes and reaches listeners.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    let request = pi_js_compat::js_json!({"operation":"collect","events":[null,false,17,"raw",[],{}],"reply":{"type":"response","success":true,"command":"prompt"}});
    let expected = source.request(&request);
    assert_eq!(native(&request).await, expected);
}

#[tokio::test]
async fn rpc_client_honors_the_runtime_executable_and_argument_order() {
    // Frozen rpc-client.ts:98-106 uses executablePath, then nodeArgs/cliPath/mode/provider/model/args.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runtime.py");
    std::fs::write(&path, "#!/usr/bin/python3\nimport json,sys,os\nfor line in sys.stdin:\n c=json.loads(line)\n print(json.dumps({'id':c['id'],'type':'response','command':c['type'],'success':True,'data':{'argv':sys.argv[1:],'env':os.environ['PI_RPC_TOKEN']}}),flush=True)\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let client = RpcClient::new(RpcClientOptions {
        executable_path: Some(path.to_str().unwrap().into()),
        cli_path: Some("entry".into()),
        node_args: vec!["runtime-flag".into()],
        provider: Some("provider".into()),
        model: Some("model".into()),
        args: vec!["extra".into()],
        env: std::collections::BTreeMap::from([("PI_RPC_TOKEN".into(), "private".into())]),
        ..Default::default()
    });
    client.start().await.unwrap();
    let response = client.send(RpcCommandBody::GetState).await.unwrap();
    client.stop().await.unwrap();
    assert_eq!(
        response.get("data"),
        Some(
            &pi_js_compat::js_json!({"argv":["runtime-flag","entry","--mode","rpc","--provider","provider","--model","model","extra"],"env":"private"})
        )
    );
}

#[tokio::test]
async fn rpc_client_accepts_declared_model_shapes_without_requiring_full_models() {
    // Frozen client :262-288 and :46-51 declare reference/info shapes, not full Models.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    for (method, data) in [
        (
            "model",
            r#"{"extra":"\ud800","id":"i\udfff","provider":"p\ud800","tail":{"raw":17}}"#,
        ),
        (
            "cycle",
            r#"{"model":{"id":"i","unknown":false,"provider":"p"},"thinkingLevel":"high","isScoped":true}"#,
        ),
        (
            "models",
            r#"{"models":[{"contextWindow":4096.5,"extra":"\ud800","reasoning":true,"provider":"p","id":"i"}]}"#,
        ),
    ] {
        let request = pi_js_compat::js_json!({"operation":"typed","method":method,"reply":{"type":"response","command":"unused","success":true,"data":js_json::parse_lossless(data).unwrap()}});
        let expected = source.request(&request);
        assert_eq!(native(&request).await, expected, "{method}");
    }
}

#[tokio::test]
async fn rpc_client_error_construction_follows_external_success_truthiness() {
    // Frozen :631-639 uses Boolean(success), then new Error(error), not DTO bool/string parsing.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    for reply in [
        r#"{"type":"response","success":[],"data":{"cancelled":true}}"#,
        r#"{"type":"response","success":false,"error":null}"#,
        r#"{"type":"response","success":0,"error":["a",null,"\ud800"]}"#,
        r#"{"type":"response","success":false,"error":{"toString":0}}"#,
    ] {
        let request = pi_js_compat::js_json!({"operation":"typed","method":"new","reply":js_json::parse_lossless(reply).unwrap()});
        let expected = source.request(&request);
        assert_eq!(native(&request).await, expected, "{reply}");
    }
}

#[tokio::test]
async fn rpc_client_settles_on_the_first_agent_end_even_when_retrying() {
    // Frozen rpc-client.ts:491-501 uses only event.type, not willRetry.
    let request =
        pi_js_compat::js_json!({"operation":"collect","reply":{"type":"response","command":"prompt","success":true}});
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    let expected = source.request(&request);
    assert_eq!(native(&request).await, expected);
}

#[tokio::test]
async fn rpc_client_send_returns_the_unchecked_external_envelope() {
    // Frozen :523-535 resolves the parsed object directly, with no DTO conversion.
    let mut source = oracle::TsOracle::spawn("rpc-client-oracle.mts", SOURCE);
    for reply in [
        r#"{"type":"response","command":"get_state","success":true,"data":null,"extra":{"text":"\ud800","2":2,"1":1}}"#,
        r#"{"type":"response","command":false,"success":"yes","error":{"raw":"\udfff"}}"#,
        r#"{"type":"response","success":0,"data":[false,17,null]}"#,
    ] {
        let request = pi_js_compat::js_json!({"operation":"send","reply":js_json::parse_lossless(reply).unwrap()});
        let expected = source.request(&request);
        assert_eq!(native(&request).await, expected, "{reply}");
    }
}
