use nix::sys::eventfd::{eventfd, EfdFlags};
use nix::unistd::{close, read, write};
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;

macro_rules! exits {
	( $code:expr ) => {
		::std::process::exit($code)
	};

	( $code :expr, $fmt:expr $( , $arg:expr )* ) => {{
        eprintln!($fmt $( , $arg )*);
		::std::process::exit($code)
	}};
}

pub fn event() -> (Notifier, Waiter) {
    let ef = eventfd(0, EfdFlags::EFD_SEMAPHORE)
        .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to create event fd: {}", e));
    let ef = Rc::new(ef);
    (Notifier { fd: ef.clone() }, Waiter { fd: ef })
}

pub struct Notifier {
    fd: Rc<RawFd>,
}

impl Notifier {
    pub fn notify(&self) {
        write(*self.fd, &1u64.to_ne_bytes())
            .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to write event fd: {}", e));
    }
}

impl Drop for Notifier {
    fn drop(&mut self) {
        if Rc::strong_count(&self.fd) == 1 {
            close(*self.fd).unwrap_or_else(|e| {
                exits!(
                    exitcode::OSERR,
                    "Fail to close notifer, fd {}: {}",
                    *self.fd,
                    e
                )
            })
        }
    }
}

impl AsRawFd for Notifier {
    fn as_raw_fd(&self) -> RawFd {
        *self.fd
    }
}

pub struct Waiter {
    fd: Rc<RawFd>,
}

impl Waiter {
    pub fn wait(&self) {
        let mut buf = [0; 8];
        read(*self.fd, &mut buf)
            .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to read event fd: {}", e));
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        if Rc::strong_count(&self.fd) == 1 {
            close(*self.fd).unwrap_or_else(|e| {
                exits!(
                    exitcode::OSERR,
                    "Fail to close waiter, fd {}: {}",
                    *self.fd,
                    e
                )
            })
        }
    }
}

impl AsRawFd for Waiter {
    fn as_raw_fd(&self) -> RawFd {
        *self.fd
    }
}
