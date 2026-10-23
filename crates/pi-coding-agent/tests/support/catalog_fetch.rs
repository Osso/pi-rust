use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_js_compat::{
    abort::AbortSignal,
    js_json::{self, JsValue},
};
struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Ok) })
    }
}
pub struct CatalogFetch(pub JsValue);
impl Fetch for CatalogFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        assert_eq!(request.url, "https://openrouter.ai/api/v1/models");
        assert!(signal.is_some());
        let bytes = js_json::stringify_lossless(&self.0).into_bytes();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: String::new(),
                headers: vec![],
                body: Box::new(Body(Some(bytes))),
            })
        })
    }
}
