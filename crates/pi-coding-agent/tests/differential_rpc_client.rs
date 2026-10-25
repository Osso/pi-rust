#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::modes::rpc::{RpcClient, RpcClientOptions, RpcCommandBody};
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
    let result = if request.get("operation").and_then(JsValue::as_string) == Some(&"send".into()) {
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
        Err(error) => pi_js_compat::js_json!({"ok":false,"error":{"name":"Error","message":error.to_string()}}),
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
