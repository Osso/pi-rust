//! Own UI-local factory results on a real UI thread, not in a Send UI context.
use std::future::Future;
use std::pin::Pin;

pub fn run<T: Send + 'static>(
    local_tasks: bool,
    work: impl FnOnce() -> Pin<Box<dyn Future<Output = T>>> + Send + 'static,
) -> pi_ai::request::BoxFuture<'static, T> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = if local_tasks {
            runtime.block_on(tokio::task::LocalSet::new().run_until(async move { work().await }))
        } else {
            runtime.block_on(work())
        };
        let _ = sender.send(());
        result
    });
    Box::pin(async move {
        let _ = receiver.await;
        worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}
