//! Synchronous fixture bridge; component ownership never crosses threads.
use pi_coding_agent::core::extensions::*;
use std::sync::mpsc::{Sender, channel};

enum Request {
    Mount(Option<(ExtensionUiFooterFactory, ExtensionUiFooterContext)>, Sender<()>),
    Render(usize, Sender<Vec<String>>),
    Stop,
}

pub struct FooterThread {
    sender: Sender<Request>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Default for FooterThread {
    fn default() -> Self {
        let (sender, receiver) = channel();
        let worker = std::thread::spawn(move || {
            let mut component: Option<Box<dyn ExtensionUiComponent>> = None;
            while let Ok(request) = receiver.recv() {
                match request {
                    Request::Mount(factory, reply) => {
                        if let Some(previous) = component.take() {
                            previous.dispose();
                        }
                        component = factory.map(|(factory, context)| factory(context));
                        reply.send(()).unwrap();
                    }
                    Request::Render(width, reply) => {
                        let component = component.as_ref().expect("mounted footer");
                        component.invalidate();
                        reply.send(component.render(width)).unwrap();
                    }
                    Request::Stop => break,
                }
            }
        });
        Self {
            sender,
            worker: Some(worker),
        }
    }
}

impl FooterThread {
    pub fn mount(&self, factory: Option<(ExtensionUiFooterFactory, ExtensionUiFooterContext)>) {
        let (sender, receiver) = channel();
        self.sender.send(Request::Mount(factory, sender)).unwrap();
        receiver.recv().unwrap();
    }
    pub fn render(&self, width: usize) -> Vec<String> {
        let (sender, receiver) = channel();
        self.sender.send(Request::Render(width, sender)).unwrap();
        receiver.recv().unwrap()
    }
}

impl Drop for FooterThread {
    fn drop(&mut self) {
        self.sender.send(Request::Stop).unwrap();
        self.worker.take().unwrap().join().unwrap();
    }
}
