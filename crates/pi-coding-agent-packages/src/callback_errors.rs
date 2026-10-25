//! Callback conversions live with the types they convert.

impl From<crate::core::package_manager::PackageManagerError> for pi_ai::request::CallbackError {
    fn from(error: crate::core::package_manager::PackageManagerError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}
