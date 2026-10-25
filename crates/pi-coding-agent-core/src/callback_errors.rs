//! Callback conversions live with the core types they convert.

impl From<crate::core::package_manager::PackageManagerError> for pi_ai::request::CallbackError {
    fn from(error: crate::core::package_manager::PackageManagerError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}

impl From<crate::modes::interactive::theme::ThemeError> for pi_ai::request::CallbackError {
    fn from(error: crate::modes::interactive::theme::ThemeError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}

impl From<crate::core::multi_agent_store::MultiAgentStoreError> for pi_ai::request::CallbackError {
    fn from(error: crate::core::multi_agent_store::MultiAgentStoreError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}
