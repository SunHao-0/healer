use crate::BackgroundIoHandle;
use std::io::BufReader;
use std::{
    fs::File,
    io::BufRead,
    os::unix::prelude::{FromRawFd, IntoRawFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

pub fn read_background<T: IntoRawFd>(f: T, debug: bool) -> BackgroundIoHandle {
    let fd = f.into_raw_fd();
    let f = unsafe { File::from_raw_fd(fd) };
    let buf = Arc::new(Mutex::new(Vec::with_capacity(4096)));
    let finished = Arc::new(AtomicBool::new(false));
    let buf1 = Arc::clone(&buf);
    let finished1 = Arc::clone(&finished);

    std::thread::spawn(move || {
        let mut buf = String::with_capacity(4096);
        let mut reader = BufReader::new(f);
        while let Ok(sz) = reader.read_line(&mut buf) {
            if sz == 0 {
                break;
            }
            let mut shared_buf = buf1.lock().unwrap();
            shared_buf.extend(buf[..sz].as_bytes());
            if debug {
                print!("{}", &buf[..sz]);
            }
            buf.clear();
        }
        finished1.store(true, Ordering::Relaxed);
    });

    BackgroundIoHandle::new(buf, finished)
}
