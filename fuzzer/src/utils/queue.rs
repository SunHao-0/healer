use std::collections::VecDeque;
use tokio::sync::Mutex;

pub struct CQueue<T> {
    inner: Mutex<VecDeque<T>>,
}

impl<T> Default for CQueue<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
        }
    }
}

impl<T> CQueue<T> {
    pub async fn push(&self, v: T) {
        let mut inner = self.inner.lock().await;
        inner.push_back(v);
    }

    pub async fn pop(&self) -> Option<T> {
        let mut inner = self.inner.lock().await;
        inner.pop_front()
    }

    pub async fn len(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.len()
    }

    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.is_empty()
    }
}

impl<T> From<Vec<T>> for CQueue<T> {
    fn from(vals: Vec<T>) -> Self {
        Self {
            inner: Mutex::new(VecDeque::from(vals)),
        }
    }
}
