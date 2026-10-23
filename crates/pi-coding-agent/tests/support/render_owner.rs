use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::{ToolRenderCompletionExecutor, ToolRenderState};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// Test-only owner transport: production uses the existing UiRequest channel.
/// Completion futures are polled only when this owner explicitly yields.
pub struct RenderOwner {
    sender: mpsc::Sender<BoxFuture<'static, ()>>,
    receiver: mpsc::Receiver<BoxFuture<'static, ()>>,
    runtime: tokio::runtime::Runtime,
    thread: std::thread::ThreadId,
}
impl Default for RenderOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOwner {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(),
            thread: std::thread::current().id(),
        }
    }
    pub fn executor(&self) -> ToolRenderCompletionExecutor {
        let sender = self.sender.clone();
        Arc::new(move |completion| {
            sender.send(completion).expect("test renderer owner remains alive");
        })
    }
    pub fn bind(&self, state: &ToolRenderState) {
        state.set_completion_executor(Some(self.executor()));
    }
    fn run_completion(&self, completion: BoxFuture<'static, ()>) {
        assert_eq!(
            std::thread::current().id(),
            self.thread,
            "completion must run on owner thread"
        );
        self.runtime.block_on(completion);
    }
    pub fn wait_for(&self, ready: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let completion = self
                .receiver
                .recv_timeout(remaining)
                .expect("renderer completion timed out");
            self.run_completion(completion);
        }
    }
    pub fn yield_for(&self, duration: Duration) {
        let deadline = Instant::now() + duration;
        loop {
            match self
                .receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(completion) => self.run_completion(completion),
                Err(mpsc::RecvTimeoutError::Timeout) => return,
                Err(error) => panic!("renderer owner transport: {error}"),
            }
        }
    }
}
