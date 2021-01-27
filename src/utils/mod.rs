use std::io;
use std::sync::{Arc, Mutex};

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    task::JoinHandle,
};
use tokio_rt::runtime;

pub mod tokio_rt;

macro_rules! fxhashmap {
    ($($key:expr => $value:expr,)+) => { fxhashmap!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::rustc_hash::FxHashMap::default();
            $(
                let _ = _map.insert($key, $value);
            )*
            _map.shrink_to_fit();
            _map
        }
    };
}

pub(crate) fn to_boxed_str<T: AsRef<str>>(s: T) -> Box<str> {
    let t = s.as_ref();
    String::into_boxed_str(t.to_string())
}

pub struct LogReader {
    buf: Arc<Mutex<Vec<u8>>>,
    handle: JoinHandle<Option<io::Error>>,
    finished: bool,
    err: Option<io::Error>,
}

impl LogReader {
    /// Spawns a new task for logging.
    pub fn new<R: Unpin + Send + AsyncRead + 'static>(mut file: R) -> Self {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let buf0 = Arc::clone(&buf);

        let handle = runtime().spawn(async move {
            const DEFAULT_BUF_SZ: usize = 4096;
            let mut buf = Vec::with_capacity(DEFAULT_BUF_SZ);
            buf.resize_with(DEFAULT_BUF_SZ, Default::default);
            loop {
                match file.read(&mut buf[..]).await {
                    Ok(bytes_read) => {
                        let mut shared_buf = buf0.lock().unwrap();
                        if bytes_read == 0 {
                            // EOF reached.
                            break None;
                        }
                        // Push content to the buffer.
                        shared_buf.extend(&buf[..bytes_read]);
                    }
                    Err(e) => {
                        break Some(e);
                    }
                }
            }
        });
        LogReader {
            buf,
            handle,
            finished: false,
            err: None,
        }
    }

    pub fn current_buffer(&self) -> Vec<u8> {
        self.buf.lock().unwrap().clone()
    }

    pub fn current_string(&self) -> String {
        String::from_utf8_lossy(&self.current_buffer()).into_owned()
    }

    pub fn clear(&self) {
        self.buf.lock().unwrap().clear();
    }

    pub fn read_all(&mut self) -> (Vec<u8>, &Option<io::Error>) {
        if !self.finished {
            self.err = runtime().block_on(&mut self.handle).unwrap();
            self.finished = true;
        }
        let val = self.buf.lock().unwrap().clone();
        (val, &self.err)
    }

    pub fn read_to_string(&mut self) -> (String, &Option<io::Error>) {
        let (val, err) = self.read_all();
        (String::from_utf8_lossy(&val).into_owned(), err)
    }
}

#[cfg(not(target_os = "windows"))]
use std::os::unix::io::{FromRawFd, IntoRawFd};
#[cfg(target_os = "windows")]
use std::os::windows::io::{FromRawHandle, IntoRawHandle};

#[cfg(not(target_os = "windows"))]
pub fn into_async_file<F: IntoRawFd>(f: F) -> tokio::fs::File {
    let fd = f.into_raw_fd();
    let std_file = unsafe { std::fs::File::from_raw_fd(fd) };
    tokio::fs::File::from_std(std_file)
}

#[cfg(target_os = "windows")]
pub fn into_async_file<F: IntoRawHandle>(f: F) -> tokio::fs::File {
    let fd = f.into_raw_handle();
    let std_file = unsafe { std::fs::File::from_raw_handle(fd) };
    tokio::fs::File::from_std(std_file)
}
