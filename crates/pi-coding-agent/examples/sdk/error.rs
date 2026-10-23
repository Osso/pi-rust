//! Native CLI boundary: preserve the callback source and use shared Node formatting.
use pi_ai::request::CallbackError;
use pi_js_compat::js_json::encode_node_utf8;

pub struct ExampleError(pub CallbackError);
impl ExampleError {
    pub fn new(error: impl Into<CallbackError>) -> Self {
        Self(error.into())
    }
}
impl std::fmt::Debug for ExampleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rendered = pi_coding_agent::utils::uncaught::format_uncaught(&self.0);
        // Rust Result termination supplies the ordinary Error prefix itself.
        let rendered = if self.0.name == "Error" {
            rendered.strip_prefix("Error: ").unwrap_or(rendered)
        } else {
            rendered
        };
        formatter.write_str(&encode_node_utf8(&rendered))
    }
}
impl std::fmt::Display for ExampleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for ExampleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}
