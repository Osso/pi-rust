use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::js_json::JsString;
use pi_orchestrator::{
    radius::{RadiusOptions, RadiusPresence},
    storage::Storage,
};
use std::sync::Arc;
struct Fake;
struct Body(Option<Vec<u8>>, bool);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move {
            if self.1 {
                Some(Err(CallbackError::new("body exploded")))
            } else {
                self.0.take().map(Ok)
            }
        })
    }
}
impl Fetch for Fake {
    fn fetch(
        &self,
        r: HttpRequest,
        _: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: Box::new(Body(
                    Some(b"{\"id\":\"m\",\"heartbeatIntervalMs\":1000000,\"expiresInMs\":2000000}".to_vec()),
                    !r.url.ends_with("register"),
                )),
            })
        })
    }
}
#[tokio::test]
async fn successful_disconnect_ignores_unreadable_body_like_frozen() {
    let source = "/home/osso/.worktrees/pi-rust-source-4f9e6f8a4";
    let d = std::path::PathBuf::from(std::env::var("TMPDIR").unwrap()).join("radius-body");
    std::fs::create_dir_all(&d).unwrap();
    let script = d.join("oracle.mjs");
    std::fs::write(&script,format!("process.env.PI_RADIUS_API_KEY='key';globalThis.fetch=async(url)=>String(url).endsWith('register')?new Response(JSON.stringify({{id:'m',heartbeatIntervalMs:1000000,expiresInMs:2000000}})):new Response(new ReadableStream({{start(c){{c.error(new Error('body exploded'));}}}}));const {{RadiusPresence}}=await import('{source}/packages/orchestrator/src/radius.ts');const r=new RadiusPresence();await r.start();await r.stop();console.log('ok');")).unwrap();
    let output = tokio::process::Command::new("node")
        .arg("--import")
        .arg(format!("{source}/node_modules/tsx/dist/loader.mjs"))
        .arg(&script)
        .env("TSX_TSCONFIG_PATH", format!("{source}/tsconfig.json"))
        .env("PI_ORCHESTRATOR_DIR", d.join("ts"))
        .output()
        .await
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = String::from_utf8(output.stdout).unwrap();
    let presence = RadiusPresence::new(
        Storage(JsString::from((d.join("rust")).to_str().expect("utf-8 test path"))),
        Arc::new(Fake),
        RadiusOptions {
            env: Arc::new(|| [("PI_RADIUS_API_KEY".into(), "key".into())].into()),
            credential: Arc::new(|| None),
            now: Arc::new(|| "time".into()),
            random: Arc::new(|| 0.5),
            hostname: "host".into(),
            platform: "linux".into(),
            arch: "x64".into(),
            pid: 1,
        },
    );
    presence.start(None).await.unwrap();
    let actual = presence.stop().await;
    println!("successful disconnect TS={expected:?} Rust={actual:?}");
    assert!(actual.is_ok());
}
