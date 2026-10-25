use crate::shared_oracle as oracle;

use pi_coding_agent::modes::rpc::attach_jsonl_line_reader;
use pi_js_compat::js_json::JsValue;
use tokio::io::AsyncRead;

struct BrokenRead;
impl AsyncRead for BrokenRead {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::from_raw_os_error(
            rustix::io::Errno::IO.raw_os_error(),
        )))
    }
}

fn source_error(operation: &str) -> JsValue {
    let mut source = oracle::TsOracle::spawn(
        "rpc-stream-errors-oracle.mts",
        "packages/coding-agent/src/modes/rpc/jsonl.ts",
    );
    source.request(&pi_js_compat::js_json!({"operation":operation}))
}

#[tokio::test]
async fn line_callback_rejection_is_uncaught_like_the_frozen_data_listener() {
    // Frozen jsonl.ts:25,38 has no catch around onLine.
    let expected = source_error("callback");
    let reader = attach_jsonl_line_reader(std::io::Cursor::new(b"line\n".to_vec()), |_| {
        Err(pi_js_compat::node_error::NativeNodeError::from_constructor(
            pi_js_compat::node_error::NativeErrorClass::TypeError,
            pi_js_compat::js_json::JsString::from("callback-")
                + pi_js_compat::js_json::JsString::from_utf16(vec![0xd800]),
        )
        .with_property("code", JsValue::String("ECALL".into()))
        .into())
    });
    let task = tokio::spawn(async move { reader.finished().await });
    let panic = task
        .await
        .expect_err("line callback rejection must be uncaught")
        .into_panic();
    let callback = panic
        .downcast_ref::<pi_ai::request::CallbackError>()
        .expect("shared callback carrier");
    assert_eq!(callback.thrown_value().value(), expected);
}

#[tokio::test]
async fn unhandled_stream_read_failure_reaches_native_uncaught_authority() {
    // Frozen jsonl.ts:49-50 installs data/end only, not an error handler.
    let expected = source_error("read");
    let reader = attach_jsonl_line_reader(BrokenRead, |_| Ok(()));
    let task = tokio::spawn(async move { reader.finished().await });
    let panic = task
        .await
        .expect_err("read errors must not disappear in an unobserved task Result")
        .into_panic();
    let native = panic
        .downcast_ref::<pi_js_compat::node_error::NativeNodeError>()
        .expect("native uncaught error");
    assert_eq!(native.value(), expected);
}
