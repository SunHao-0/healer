use crate::BackgroundIoHandle;
use std::{
    fs::File,
    io::Read,
    os::unix::prelude::{FromRawFd, IntoRawFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub fn read_background<T: IntoRawFd>(f: T) -> BackgroundIoHandle {
    let fd = f.into_raw_fd();
    let mut f = unsafe { File::from_raw_fd(fd) };
    let buf = Arc::new(Mutex::new(Vec::with_capacity(4096)));
    let finished = Arc::new(AtomicBool::new(false));
    let buf1 = Arc::clone(&buf);
    let finished1 = Arc::clone(&finished);

    std::thread::spawn(move || {
        let buf = vec![0_u8; 1024 * 128];
        let mut buf = buf.into_boxed_slice();

        while let Ok(sz) = f.read(&mut buf[..]) {
            if sz == 0 {
                break;
            }
            let mut shared_buf = buf1.lock().unwrap();
            shared_buf.extend(&buf[..sz]);
        }
        finished1.store(true, Ordering::Relaxed);
    });

    BackgroundIoHandle::new(buf, finished)
}
