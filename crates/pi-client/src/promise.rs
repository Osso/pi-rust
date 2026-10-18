use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::sync::oneshot;

use crate::errors::PiError;

/// The resolving half of a one-shot promise.
pub(crate) struct Resolver<T> {
    sender: Option<oneshot::Sender<Result<T, PiError>>>,
}

/// The awaiting half of a one-shot promise.
pub struct Promise<T> {
    receiver: oneshot::Receiver<Result<T, PiError>>,
}

/// Creates a one-shot promise with its resolver.
pub(crate) fn create_promise_resolvers<T>() -> (Resolver<T>, Promise<T>) {
    let (sender, receiver) = oneshot::channel();
    (Resolver { sender: Some(sender) }, Promise { receiver })
}

impl<T> Resolver<T> {
    pub(crate) fn resolve(mut self, value: T) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Ok(value));
        }
    }

    pub(crate) fn reject(mut self, error: PiError) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Err(error));
        }
    }
}

impl<T> Future for Promise<T> {
    type Output = Result<T, PiError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.receiver).poll(context) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => Poll::Ready(Err(PiError::disconnected("Pi client is disconnected"))),
            Poll::Pending => Poll::Pending,
        }
    }
}
