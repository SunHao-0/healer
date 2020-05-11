use crate::utils::{event, Notifier, Waiter};
use crate::Config;
use byte_slice_cast::*;
use byteorder::*;
use core::prog::Prog;
use core::target::Target;
use nix::fcntl::{fcntl, FcntlArg};
use nix::poll::{poll, PollFd, PollFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag};
use nix::unistd::{dup2, fork, ForkResult, Pid};
use os_pipe::PipeWriter;
use rand::random;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{read_to_string, write};
use std::io::Read;
use std::mem;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;

pub fn fork_exec(p: Prog, t: &Target, conf: &Config) -> ExecResult {
    if conf.concurrency || random::<f64>() < 0.0025 {
        bg_run(&p, t);
    }
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
            sync_exec(&p, t, &mut wp, waiter, conf);
            exit(exitcode::OK)
        }
        Ok(ForkResult::Parent { child }) => {
            drop(wp);
            drop(err_wp);
            drop(waiter);

            watch(child, &mut rp, &mut err_rp, notifer, conf)
        }
        Err(e) => exits!(exitcode::OSERR, "Fail to fork: {}", e),
    }
}

fn bg_run(p: &Prog, t: &Target) {
    #[cfg(feature = "interprete")]
    use interprete::bg_exec;
    #[cfg(feature = "jit")]
    use jit::bg_exec;
    #[cfg(feature = "syscall")]
    use syscall::bg_exec;

    match fork() {
        Ok(ForkResult::Child) => match fork() {
            Ok(ForkResult::Child) => {
                for _ in 0..3 {
                    let mut wait_call = p.calls.len();
                    match fork() {
                        Ok(ForkResult::Child) => {
                            bg_exec(p, t);
                            exit(0);
                        }
                        Ok(ForkResult::Parent { child }) => loop {
                            sleep(Duration::from_millis(100));
                            match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
                                Ok(status) => {
                                    if status.pid().is_some() {
                                        kill_and_wait(child);
                                        break;
                                    }
                                }
                                Err(_) => {
                                    kill_and_wait(child);
                                    break;
                                }
                            }
                            wait_call -= 1;
                            if wait_call == 0 {
                                kill_and_wait(child);
                                break;
                            }
                        },
                        Err(_) => exit(0),
                    }
                }
                exit(0);
            }
            _ => exit(0),
        },
        Ok(ForkResult::Parent { child }) => {
            waitpid(child, None).unwrap();
        }
        Err(e) => exits!(exitcode::OSERR, "Fail to fork: {}", e),
    }
}

fn watch<T: Read + AsRawFd>(
    child: Pid,
    data: &mut T,
    err: &mut T,
    notifer: Notifier,
    conf: &Config,
) -> ExecResult {
    let mut fds = vec![
        PollFd::new(data.as_raw_fd(), PollFlags::POLLIN),
        PollFd::new(err.as_raw_fd(), PollFlags::POLLIN),
    ];
    let mut covs = Vec::new();
    let wait_timeout = if conf.memleak_check { 3000 } else { 1000 };

    loop {
        match poll(&mut fds, wait_timeout) {
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
                            if conf.memleak_check {
                                if let Some(leak) = check_leak(child.to_string()) {
                                    return ExecResult::Failed(Reason(format!(
                                        "CRASH-MEMLEAK:\n{}",
                                        leak
                                    )));
                                }
                            }
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

const MEM_LEAK: &str = "/sys/kernel/debug/kmemleak";

fn mem_leak_clear() {
    write(MEM_LEAK, "clear").unwrap();
}

fn check_leak(_: String) -> Option<String> {
    use std::fmt::Write;
    let mut executor_leak = String::new();
    write(MEM_LEAK, "scan").unwrap();
    let leaks = read_to_string(MEM_LEAK).unwrap();
    let p = PathBuf::from(std::env::args().next().unwrap());
    let p_name = p.file_name().unwrap().to_str().unwrap();
    if !leaks.is_empty() {
        let leaks_all = parse_leak(&leaks);
        for l in leaks_all.iter() {
            if l.contains(p_name) {
                writeln!(executor_leak, "{}", l).unwrap();
            }
        }
    }
    write(MEM_LEAK, "clear").unwrap();
    if executor_leak.is_empty() {
        None
    } else {
        Some(executor_leak)
    }
}

fn parse_leak(leaks_src: &str) -> Vec<&str> {
    assert!(!leaks_src.is_empty());
    let mut ret = Vec::new();

    let mut leaks = leaks_src.match_indices("unreferenced object");
    let (mut prev, _) = leaks.next().unwrap();
    loop {
        if let Some((crt, _)) = leaks.next() {
            ret.push(&leaks_src[prev..crt]);
            prev = crt
        } else {
            ret.push(&leaks_src[prev..]);
            break;
        }
    }
    ret
}

fn kill_and_wait(child: Pid) {
    if let Err(e) = kill(child, Some(Signal::SIGKILL)) {
        eprintln!(
            // exitcode::OSERR,
            "Fail to kill subprocess(pid {}): {}",
            child,
            e
        );
    }
    if let Err(e) = waitpid(child, None) {
        eprintln!(
            // exitcode::OSERR,
            "Fail to wait subprocess(pid {}): {}",
            child,
            e
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecResult {
    Ok(Vec<Vec<usize>>),
    Failed(Reason),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Reason(pub String);

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}

#[cfg(feature = "jit")]
pub mod jit;

#[cfg(feature = "syscall")]
pub mod syscall;

pub fn sync_exec(p: &Prog, t: &Target, out: &mut PipeWriter, waiter: Waiter, conf: &Config) {
    if conf.memleak_check {
        mem_leak_clear();
    }

    #[cfg(feature = "jit")]
    use jit::exec;
    #[cfg(feature = "syscall")]
    use syscall::exec;
    exec(p, t, out, waiter);
}
