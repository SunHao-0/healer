use crate::cover;
use crate::picoc::Picoc;
use crate::utils::{event, Notifier, Waiter};
use byte_slice_cast::*;
use byteorder::WriteBytesExt;
use byteorder::*;
use core::c::iter_trans;
use core::prog::Prog;
use core::target::Target;
use nix::fcntl::{fcntl, FcntlArg};
use nix::poll::{poll, PollFd, PollFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::waitpid;
use nix::unistd::{dup2, fork, ForkResult, Pid};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};
use std::mem;
use std::os::unix::io::AsRawFd;
use std::process::exit;

pub fn fork_exec(p: Prog, t: &Target) -> ExecResult {
    // transfer usefull data
    let (mut rp, mut wp) = os_pipe::pipe()
        .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to create date pipe : {}", e));
    fcntl(wp.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(1024 * 1024)).unwrap_or_else(|e| {
        exits!(
            exitcode::OSERR,
            "Fail to set buf size for data pipe to {} :{}",
            1024 * 1024,
            e
        )
    });

    // collect err msg
    let (mut err_rp, err_wp) = os_pipe::pipe()
        .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to create err pipe : {}", e));
    // sync data transfer
    let (notifer, waiter) = event();

    match fork() {
        Ok(ForkResult::Child) => {
            drop(rp);
            drop(err_rp);
            drop(notifer);

            dup2(err_wp.as_raw_fd(), 2).unwrap_or_else(|e| {
                exits!(
                    exitcode::OSERR,
                    "Fail to redirect stderr to {}: {}",
                    err_wp.as_raw_fd(),
                    e
                )
            });
            dup2(err_wp.as_raw_fd(), 1).unwrap_or_else(|e| {
                exits!(
                    exitcode::OSERR,
                    "Fail to redirect stdout to {}: {}",
                    err_wp.as_raw_fd(),
                    e
                )
            });
            drop(err_wp);
            sync_exec(&p, t, &mut wp, waiter);
            exit(exitcode::OK)
        }
        Ok(ForkResult::Parent { child }) => {
            drop(wp);
            drop(err_wp);
            drop(waiter);

            watch(child, &mut rp, &mut err_rp, notifer)
        }
        Err(e) => exits!(exitcode::OSERR, "Fail to fork: {}", e),
    }
}

fn watch<T: Read + AsRawFd>(
    child: Pid,
    data: &mut T,
    err: &mut T,
    notifer: Notifier,
) -> ExecResult {
    let mut fds = vec![
        PollFd::new(data.as_raw_fd(), PollFlags::POLLIN),
        PollFd::new(err.as_raw_fd(), PollFlags::POLLIN),
    ];
    let mut covs = Vec::new();

    loop {
        match poll(&mut fds, 1000) {
            Ok(0) => {
                // timeout
                kill_and_wait(child);
                return if covs.is_empty() {
                    ExecResult::Failed(Reason(String::from("Time out")))
                } else {
                    covs.shrink_to_fit();
                    ExecResult::Ok(covs)
                };
            }
            Ok(_) => {
                if let Some(revents) = fds[1].revents() {
                    if !revents.is_empty() {
                        kill_and_wait(child);

                        let mut err_msg = Vec::new();
                        err.read_to_end(&mut err_msg).unwrap();
                        if covs.is_empty() {
                            return ExecResult::Failed(Reason(String::from_utf8(err_msg).unwrap()));
                        } else {
                            covs.shrink_to_fit();
                            return ExecResult::Ok(covs);
                        }
                    }
                }

                // Data pipe is ok
                if let Some(revents) = fds[0].revents() {
                    if revents.contains(PollFlags::POLLIN) {
                        let len = data.read_u32::<NativeEndian>().unwrap_or_else(|e| {
                            exits!(exitcode::OSERR, "Fail to read length of covs: {}", e)
                        });
                        let len = len as usize * mem::size_of::<usize>();
                        let mut buf = bytes::BytesMut::with_capacity(len);
                        unsafe {
                            buf.set_len(len);
                        }
                        data.read_exact(&mut buf).unwrap_or_else(|e| {
                            exits!(exitcode::IOERR, "Fail to read covs(len {}): {}", len, e)
                        });
                        notifer.notify();

                        let mut new_cov = Vec::from(buf.as_ref().as_slice_of::<usize>().unwrap());
                        new_cov.shrink_to_fit();
                        covs.push(new_cov);
                    }
                }
            }
            Err(e) => exits!(exitcode::SOFTWARE, "Fail to poll: {}", e),
        }
    }
}

fn kill_and_wait(child: Pid) {
    kill(child, Some(Signal::SIGKILL)).unwrap_or_else(|e| {
        exits!(
            exitcode::OSERR,
            "Fail to kill subprocess(pid {}): {}",
            child,
            e
        )
    });
    waitpid(child, None).unwrap_or_else(|e| {
        exits!(
            exitcode::OSERR,
            "Fail to wait subprocess(pid {}) to terminate: {}",
            child,
            e
        )
    });
}

pub fn sync_exec<T: Write>(p: &Prog, t: &Target, out: &mut T, waiter: Waiter) {
    let mut picoc = Picoc::default();
    let mut handle = cover::open();
    let mut success = false;

    for stmts in iter_trans(p, t) {
        let covs = handle.collect(|| {
            success = picoc.execute(stmts.to_string());
        });
        if success {
            send_covs(covs, out);
            waiter.wait()
        } else {
            exit(exitcode::SOFTWARE)
        }
    }
}

fn send_covs<T: Write>(covs: &[usize], out: &mut T) {
    use byte_slice_cast::*;
    assert!(!covs.is_empty());

    out.write_u32::<NativeEndian>(covs.len() as u32)
        .unwrap_or_else(|e| exits!(exitcode::IOERR, "Fail to write len of covs: {}", e));
    out.write_all(covs.as_byte_slice())
        .unwrap_or_else(|e| exits!(exitcode::IOERR, "Fail to write covs: {}", e));
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecResult {
    Ok(Vec<Vec<usize>>),
    Failed(Reason),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reason(String);

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}
