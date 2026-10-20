//! Keep the Error name and message without Rust Debug quoting or JS frames.
pub struct ExampleError(pub String);
impl std::fmt::Debug for ExampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
