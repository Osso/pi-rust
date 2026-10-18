use std::sync::Arc;

use crate::harness::types::ExecutionEnv;

/// Filesystem and shell context required by the built-in execution tools.
#[derive(Clone)]
pub struct ExecutionToolContext {
    pub env: Arc<dyn ExecutionEnv>,
    pub file_mutation_queue: Arc<super::file_mutation_queue::FileMutationQueue>,
}

impl ExecutionToolContext {
    pub fn new(env: Arc<dyn ExecutionEnv>) -> Self {
        Self {
            env,
            file_mutation_queue: Arc::new(super::file_mutation_queue::FileMutationQueue::new()),
        }
    }
}
