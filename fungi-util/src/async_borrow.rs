use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::Notify;

pub struct AsyncBorrow<T> {
    t: Arc<Mutex<T>>,
    notify: Arc<Notify>,
}

impl<T> AsyncBorrow<T> {
    pub fn new(t: T) -> Self {
        let notify = Arc::new(Notify::new());
        Self {
            t: Arc::new(Mutex::new(t)),
            notify,
        }
    }

    pub async fn borrow(self, f: impl FnOnce(AsyncBorrowGuard<T>)) -> T {
        let guard = AsyncBorrowGuard {
            t: self.t.clone(), // SAFETY: arc clone is only called once here
            notify: self.notify.clone(),
        };
        f(guard);
        self.notify.notified().await;
        // SAFETY: arc clone was only called once and the guard is dropped
        assert!(Arc::strong_count(&self.t) == 1);
        let t = Arc::into_inner(self.t).unwrap();
        t.into_inner().unwrap()
    }
}

pub struct AsyncBorrowGuard<T> {
    t: Arc<Mutex<T>>,
    notify: Arc<Notify>,
}

impl<T> Drop for AsyncBorrowGuard<T> {
    fn drop(&mut self) {
        self.notify.notify_waiters();
    }
}

impl<T> AsyncBorrowGuard<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.t.lock().unwrap()
    }
}
