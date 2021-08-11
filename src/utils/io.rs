use bytes::Buf;
use std::fs::File;
use std::future::pending;
use std::io::{Read, Write};
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::sync::{Barrier, Once};
use std::{mem, slice};
use tokio::runtime::{Builder, Runtime};

use crate::utils::debug;

pub fn read_background<T: IntoRawFd>(f: T) -> BackgroundIoHandle {
    let fd = f.into_raw_fd();
    let f = unsafe { File::from_raw_fd(fd) };
    let mut f = tokio::fs::File::from_std(f);
    let buf = Arc::new(Mutex::new(Vec::with_capacity(1024)));
    let finished = Arc::new(AtomicBool::new(false));
    let buf1 = Arc::clone(&buf);
    let finished1 = Arc::clone(&finished);

    runtime().spawn(async move {
        use tokio::io::*;
        log::debug!("reading");

        let mut buf = [0_u8; 4096];
        while let Ok(sz) = f.read(&mut buf).await {
            if sz == 0 {
                break;
            }
            let mut shared_buf = buf1.lock().unwrap();
            shared_buf.extend(&buf[..sz]);
            if debug() {
                let info = String::from_utf8_lossy(&shared_buf);
                log::debug!("{}", info);
            }
        }

        if debug() {
            log::debug!("read_background exited");
        }

        finished1.store(true, Ordering::Relaxed);
    });

    BackgroundIoHandle::new(buf, finished)
}

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
        // just busy loop here
        while !self.finished.load(Ordering::Relaxed) {}
        self.current_data()
    }
}

static mut RUNTIME: Option<Runtime> = None;
static ONCE: Once = Once::new();

fn init_runtime() {
    ONCE.call_once(|| {
        // from quaff-async.
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to init tokio runtime.");
        unsafe { RUNTIME = Some(rt) }
        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let task = async move {
            barrier1.wait();
            pending::<()>().await
        };
        std::thread::Builder::new()
            .name("healer-bg-thread".into())
            .spawn(move || unsafe { RUNTIME.as_ref().unwrap() }.block_on(task))
            .expect("failed to spawn healer background thread");

        // A single-threaded runtime polls the inner future for the first thread
        // which invokes block_on. If the worker thread does not invoke "block_on"
        // before other threads, other threads can "steal" the execution. Here we
        // wait for the worker thread to be ready, which takes the job of polling
        // from now on.
        barrier.wait();
        log::info!("backgound io initalized");
    });
}

pub fn runtime() -> &'static Runtime {
    init_runtime();
    unsafe { RUNTIME.as_ref().unwrap() }
}

pub fn read_u32(buf: &mut &[u8]) -> Option<u32> {
    if buf.remaining() >= 4 {
        Some(buf.get_u32_le())
    } else {
        None
    }
}

pub fn read_u32_slice<'a>(buf: &mut &'a [u8], len: usize) -> Option<&'a [u32]> {
    let l = len * mem::size_of::<u32>();
    if l <= buf.len() {
        let ret = unsafe { slice::from_raw_parts(buf.as_ptr() as *const u32, len) };
        buf.advance(l);
        Some(ret)
    } else {
        None
    }
}

pub fn read<'a, T: Sized>(buf: &mut &'a [u8]) -> Option<&'a T> {
    let sz = mem::size_of::<T>();
    if buf.len() >= sz {
        let buf0 = &buf[0..sz];
        let v = cast_from(buf0);
        buf.advance(sz);
        Some(v)
    } else {
        None
    }
}

pub fn read_exact<T: Default + Sized, R: Read>(mut r: R) -> Result<T, std::io::Error> {
    let mut v = T::default();
    let data = cast_to_mut(&mut v);
    r.read_exact(data)?;
    Ok(v)
}

pub fn write_all<T: Sized, W: Write>(mut w: W, v: &T) -> Result<(), std::io::Error> {
    let data = cast_to(v);
    w.write_all(data)
}

pub fn cast_to<T: Sized>(v: &T) -> &[u8] {
    let ptr = (v as *const T).cast::<u8>();
    let len = mem::size_of::<T>();
    unsafe { slice::from_raw_parts(ptr, len) }
}

pub fn cast_to_mut<T: Sized>(v: &mut T) -> &mut [u8] {
    let ptr = (v as *mut T).cast::<u8>();
    let len = mem::size_of::<T>();
    unsafe { slice::from_raw_parts_mut(ptr, len) }
}

pub fn cast_from<T: Sized>(v: &[u8]) -> &T {
    assert_eq!(v.len(), mem::size_of::<T>());
    let ptr = v.as_ptr() as *const T;
    unsafe { ptr.as_ref().unwrap() }
}

pub fn cast_from_mut<T: Sized>(v: &mut [u8]) -> &mut T {
    assert_eq!(v.len(), mem::size_of::<T>());
    let ptr = v.as_ptr() as *mut T;
    unsafe { ptr.as_mut().unwrap() }
}
