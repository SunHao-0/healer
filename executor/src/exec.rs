use crate::cover;
use crate::picoc::Picoc;
use byte_slice_cast::*;
use byteorder::WriteBytesExt;
use byteorder::*;
use core::c::{iter_trans, translate};
use core::prog::Prog;
use core::target::Target;
use nix::fcntl::{fcntl, FcntlArg};
use nix::poll::{poll, PollFd, PollFlags};
use nix::unistd::{dup2, fork, read, ForkResult};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};
use std::mem;
use std::os::unix::io::AsRawFd;
use std::process::exit;

pub fn fork_exec(p: Prog, t: &Target) -> ExecResult {
    let (mut rp, mut wp) = os_pipe::pipe()
        .unwrap_or_else(|e| exits!(exitcode::OSERR, "Executor: Fail to pipe : {}", e));
    let (mut err_rp, err_wp) = os_pipe::pipe()
        .unwrap_or_else(|e| exits!(exitcode::OSERR, "Executor: Fail to pipe : {}", e));

    fcntl(wp.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(1024 * 1024)).unwrap_or_else(|e| {
        exits!(
            exitcode::OSERR,
            "Fail to set pipe size to {} :{}",
            1024 * 1024,
            e
        )
    });

    match fork() {
        Ok(ForkResult::Child) => {
            drop(rp);
            drop(err_rp);
            dup2(err_wp.as_raw_fd(), 2)
                .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to redirect: {}", e));
            drop(err_wp);

            exec(&p, t, &mut wp);

            exit(exitcode::OK)
        }
        Ok(ForkResult::Parent { child }) => {
            drop(wp);
            drop(err_wp);

            watch(&mut rp, &mut err_rp)
        }
        Err(e) => exits!(exitcode::OSERR, "Executor: Fail to fork: {}", e),
    }
}

fn watch<T: Read + AsRawFd>(data: &mut T, err: &mut T) -> ExecResult {
    let mut fds = vec![
        PollFd::new(data.as_raw_fd(), PollFlags::POLLIN),
        PollFd::new(err.as_raw_fd(), PollFlags::POLLIN),
    ];
    let mut covs = Vec::new();
    let mut len_readed = false;
    let mut len = 0;
    loop {
        match poll(&mut fds, 500) {
            Ok(0) => {
                if covs.is_empty() {
                    return ExecResult::Err(Error(String::from("time out")));
                } else {
                    covs.shrink_to_fit();
                    return ExecResult::Ok(covs);
                }
            }
            Ok(_) => {
                if let Some(PollFlags::POLLIN) = fds[1].revents() {
                    let mut err_msg = Vec::new();
                    err.read_to_end(&mut err_msg).unwrap();
                    if covs.is_empty() {
                        return ExecResult::Err(Error(String::from_utf8(err_msg).unwrap()));
                    } else {
                        covs.shrink_to_fit();
                        return ExecResult::Ok(covs);
                    }
                }
                if let Some(PollFlags::POLLHUP) = fds[1].revents() {
                    let mut err_msg = Vec::new();
                    err.read_to_end(&mut err_msg).unwrap();
                    if covs.is_empty() {
                        return ExecResult::Err(Error(String::from_utf8(err_msg).unwrap()));
                    } else {
                        covs.shrink_to_fit();
                        return ExecResult::Ok(covs);
                    }
                }
                // Data pipe is ok
                if let Some(PollFlags::POLLIN) = fds[0].revents() {
                    if len_readed {
                        let len = len as usize * mem::size_of::<usize>();
                        let mut buf = bytes::BytesMut::with_capacity(len);
                        unsafe {
                            buf.set_len(len);
                        }
                        data.read_exact(&mut buf).unwrap_or_else(|e| {
                            exits!(exitcode::IOERR, "Fail to read len {} of covs: {}", len, e)
                        });
                        let mut new_cov = Vec::from(buf.as_ref().as_slice_of::<usize>().unwrap());
                        new_cov.shrink_to_fit();
                        covs.push(new_cov);
                        len_readed = false;
                    } else {
                        len = data.read_u32::<NativeEndian>().unwrap_or_else(|e| {
                            exits!(exitcode::OSERR, "Fail to read len of covs: {}", e)
                        });
                        len_readed = true;
                    }
                }
            }
            Err(e) => exits!(exitcode::SOFTWARE, "Executor: Fail to poll: {}", e),
        }
    }
}

pub fn exec<T: Write>(p: &Prog, t: &Target, out: &mut T) {
    let mut kcov_handle = cover::open();
    let mut picoc = Picoc::default();
    let mut success = false;

    for s in iter_trans(p, t) {
        let p = s.to_string();
        let covs = kcov_handle.collect(|| {
            success = picoc.execute(p.clone());
        });
        if success {
            println!("executed cov {}:\n{}", covs.len(), p);
        } else {
            eprintln!("Fail to execute:\n{}", p);
        }
    }
}

fn send_covs<T: Write>(covs: &[usize], out: &mut T) {
    use byte_slice_cast::*;
    if !covs.is_empty() {
        out.write_u32::<NativeEndian>(covs.len() as u32)
            .unwrap_or_else(|e| exits!(exitcode::IOERR, "Fail to write len of covs: {}", e));
        out.write_all(covs.as_byte_slice())
            .unwrap_or_else(|e| exits!(exitcode::IOERR, "Fail to write covs: {}", e));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecResult {
    Ok(Vec<Vec<usize>>),
    Err(Error),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}
