//! Callback conversions live with the types they convert.

impl From<crate::modes::interactive::theme::ThemeError> for pi_ai::request::CallbackError {
    fn from(error: crate::modes::interactive::theme::ThemeError) -> Self {
        Self::from_error(error.message().clone(), error)
    }
}
