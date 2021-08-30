use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

pub mod thread;
#[cfg(features = "tokio-io")]
pub mod tokio;

#[derive(Debug)]
pub struct BackgroundIoHandle {
    buf: Arc<Mutex<Vec<u8>>>,
    finished: Arc<AtomicBool>,
}

impl BackgroundIoHandle {
    fn new(buf: Arc<Mutex<Vec<u8>>>, finished: Arc<AtomicBool>) -> Self {
        Self { buf, finished }
    }

    pub fn current_data(&self) -> Vec<u8> {
        let mut buf = self.buf.lock().unwrap();
        buf.split_off(0)
    }

    pub fn clear_current(&self) {
        let mut buf = self.buf.lock().unwrap();
        buf.clear();
    }

    pub fn wait_finish(self) -> Vec<u8> {
        while !self.finished.load(Ordering::Relaxed) {}
        self.current_data()
    }
}

impl Clone for BackgroundIoHandle {
    fn clone(&self) -> Self {
        BackgroundIoHandle {
            buf: Arc::clone(&self.buf),
            finished: Arc::clone(&self.finished),
        }
    }
}
