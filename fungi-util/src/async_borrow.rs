use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};
use tokio::sync::Notify;

pub struct AsyncBorrow<'a, T> {
    t: &'a mut T,
    notify: Arc<Notify>,
    _marker: std::marker::PhantomData<&'a mut T>,
}

impl<'a, T> AsyncBorrow<'a, T> {
    pub fn new(t: &'a mut T) -> Self {
        let notify = Arc::new(Notify::new());
        Self {
            t,
            notify,
            _marker: std::marker::PhantomData,
        }
    }

    pub async fn borrow(&mut self, f: impl FnOnce(AsyncBorrowGuard<T>)) {
        let guard = AsyncBorrowGuard {
            ptr: self.t as *mut T,
            notify: self.notify.clone(),
        };
        f(guard);
        self.notify.notified().await;
    }
}

pub struct AsyncBorrowGuard<T> {
    ptr: *mut T,
    notify: Arc<Notify>,
}

unsafe impl<T> Send for AsyncBorrowGuard<T> {}

impl<T> Drop for AsyncBorrowGuard<T> {
    fn drop(&mut self) {
        self.notify.notify_one();
    }
}

impl<T> Deref for AsyncBorrowGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for AsyncBorrowGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}
