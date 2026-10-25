//! Callback conversions live with the types they convert.

impl From<crate::core::multi_agent_store::MultiAgentStoreError> for pi_ai::request::CallbackError {
    fn from(error: crate::core::multi_agent_store::MultiAgentStoreError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}
