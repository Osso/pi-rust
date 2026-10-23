use pi_agent_core::harness::types::OperationFailure;

/// These fixtures expect a resolved err(error), never a rejected promise.
pub fn returned_error<E>(error: OperationFailure<E>) -> E {
    match error {
        OperationFailure::Returned(error) => error,
        OperationFailure::Rejected(error) => panic!("unexpected native Node rejection: {error:?}"),
    }
}
