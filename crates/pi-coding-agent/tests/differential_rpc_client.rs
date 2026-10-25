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
    let client = RpcClient::new(options);
    client.start().await.unwrap();
    let result = if request.get("operation").and_then(JsValue::as_string) == Some(&"typed".into()) {
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
        let (events, sent) = tokio::join!(
            client.collect_events(Duration::from_secs(2)),
            client.prompt("input", None)
        );
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
