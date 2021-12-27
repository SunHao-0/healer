use crate::BackgroundIoHandle;
use std::fs::File;
use std::future::pending;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, Once};
use tokio::runtime::{Builder, Runtime};

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

        barrier.wait();
        log::info!("background io initialized");
    });
}

pub fn runtime() -> &'static Runtime {
    init_runtime();
    unsafe { RUNTIME.as_ref().unwrap() }
}

pub fn read_background<T: IntoRawFd>(f: T, debug: bool) -> BackgroundIoHandle {
    let fd = f.into_raw_fd();
    let f = unsafe { File::from_raw_fd(fd) };
    let f = tokio::fs::File::from_std(f);
    let buf = Arc::new(Mutex::new(Vec::with_capacity(4096)));
    let finished = Arc::new(AtomicBool::new(false));
    let buf1 = Arc::clone(&buf);
    let finished1 = Arc::clone(&finished);

    runtime().spawn(async move {
        use tokio::io::*;
        let mut buf = String::with_capacity(4096);
        let mut reader = BufReader::new(f);

        while let Ok(sz) = reader.readline(&mut buf).await {
            if sz == 0 {
                break;
            }
            let mut shared_buf = buf1.lock().unwrap();
            shared_buf.extend(&buf[..sz]);
            if debug {
                print!("{}", &buf[..sz]);
            }
            buf.clear();
        }

        finished1.store(true, Ordering::Relaxed);
    });

    BackgroundIoHandle::new(buf, finished)
}
