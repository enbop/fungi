use std::future::Future;
use tokio::sync::oneshot;

pub struct Completer<T> {
    tx: oneshot::Sender<T>,
}

impl<T> Completer<T> {
    pub fn complete(self, res: T) {
        self.tx.send(res).ok();
    }
}

pub struct AsyncResult<T> {
    rx: oneshot::Receiver<T>,
}

impl<T> AsyncResult<T> {
    pub fn new(mut f: impl FnMut(Completer<T>)) -> Self {
        let (tx, rx) = oneshot::channel();
        f(Completer { tx });
        Self { rx }
    }

    pub async fn new_async<F: Future<Output = ()>>(mut f: impl FnMut(Completer<T>) -> F) -> Self {
        let (tx, rx) = oneshot::channel();
        f(Completer { tx }).await;
        Self { rx }
    }

    pub async fn wait(self) -> T {
        self.rx.await.unwrap()
    }
}
