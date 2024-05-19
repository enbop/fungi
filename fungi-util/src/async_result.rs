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

pub struct AsyncResult {}

impl AsyncResult {
    pub async fn new<T>(mut f: impl FnMut(Completer<T>)) -> T {
        let (tx, rx) = oneshot::channel();
        f(Completer { tx });
        rx.await.unwrap()
    }

    pub async fn new_async<T, F: Future<Output = ()>>(mut f: impl FnMut(Completer<T>) -> F) -> T {
        let (tx, rx) = oneshot::channel();
        f(Completer { tx }).await;
        rx.await.unwrap()
    }
}
