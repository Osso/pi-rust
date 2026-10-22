//! Keep the Error name and message without Rust Debug quoting or JS frames.
use pi_js_compat::js_json::{JsString, encode_node_utf8};

pub struct ExampleError(pub JsString);
impl std::fmt::Debug for ExampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&encode_node_utf8(&self.0))
    }
}
