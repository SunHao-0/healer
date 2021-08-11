#![allow(clippy::or_fun_call)]
use crate::{
    exec::serialize::serialize,
    model::Prog,
    targets::Target,
    utils::{
        debug,
        io::*,
        ssh::{scp, ssh_basic_cmd},
        stop_soon,
    },
    vm::ManageVm,
};

use std::{
    fmt::Display,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use iota::iota;
use shared_memory::{Shmem, ShmemConf};
use thiserror::Error;

use super::serialize::SerializeError;

/// Size of syz-executor input shared memory.
pub const INPUT_SHM_SZ: usize = 4 << 20;

#[derive(Debug, Clone)]
pub struct SyzExecConfig {
    pub syz_bin: PathBuf,
    pub force_setup: bool,
}

#[derive(Debug, Error)]
pub enum SyzExecConfigError {
    #[error("invalid syz-executor path: {0:?}")]
    PathInvalid(PathBuf),
    #[error("output shm should be greater than 4M, size: {0}")]
    OutShmTooSmall(usize),
    #[error("output shm should not same as input shm, size: {0}")]
    BadOutShmSize(usize),
    #[error("input shm can only be 4M currently, given size: {0}")]
    BadInShmSize(usize),
    #[error("failed to spawn syz-executor: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("test run failed: {0}")]
    TestRunFailed(String),
}

impl SyzExecConfig {
    pub fn check(&self) -> Result<(), SyzExecConfigError> {
        if !self.syz_bin.is_file() {
            return Err(SyzExecConfigError::PathInvalid(self.syz_bin.clone()));
        }
        let output = Command::new(&self.syz_bin).arg("version").output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SyzExecConfigError::TestRunFailed(format!(
                "status: {:?}, stderr: {}",
                output.status, stderr
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SyzExecError {
    #[error("failed to spawn: {0}")]
    Spawn(#[from] SyzSpawnError),
    #[error("unexpected io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("searialize: {0}")]
    Serialize(#[from] SerializeError),
    #[error("magic mismatc:, required {required}, got {got}")]
    MagicMismatch { required: u32, got: u32 },
    #[error("call info parse: {0}")]
    Parse(String),
}

/// Possible result of one execution.
pub enum SyzExecResult {
    /// Prog was executed successfully without crashing kernel or executor.
    Normal(Vec<CallExecInfo>),
    /// Prog caused kernel panic.
    Crash(Vec<u8>),
}

/// executor driver
pub struct SyzExecHandle<E: std::error::Error> {
    syz_bin_path: PathBuf,             // path to syz-executor
    remote_syz_bin_path: PathBuf,      // path to syz-executor in vm
    force_setup: bool,                 // syz-executor must be setup correctly?
    pub(super) in_shm: Option<Shmem>,  // optional input shared memory
    pub(super) out_shm: Option<Shmem>, // optional output shared memeory
    /// Input buffer for inner executor. Value is None if use shm.
    pub(super) in_mem: Option<Box<[u8]>>,
    /// Input shared memory for inner executor. Value is None if use shm.
    pub(super) out_mem: Option<Box<[u8]>>,

    pub features: Option<u64>, // enabled features, such as fault injection
    env_flags: EnvFlags,       // syz-executor env corresponding to features
    pid: u64,                  // internal pid syzkaller only, not process id

    syz: Option<Child>,                         // handle of running syz-executor
    syz_in_pipe: Option<ChildStdin>,            // input pipe for simple data, such as handshake
    syz_out_pipe: Option<ChildStdout>,          // output pipe
    syz_error_pipe: Option<BackgroundIoHandle>, // error msg from syz-executor, read by background thread

    boot_vm: bool,                                     // need boot vm or not
    vm_handle: Box<dyn ManageVm<Error = E> + 'static>, // vm handler
    vm_ip: String,                                     // vm ip
    vm_port: u16,                                      // vm port
    vm_ssh_user: String,                               // vm user
    vm_ssh_key_path: String, // path to secret key to login to vm without using password
    exec_count: u64,
}

impl<E: std::error::Error + 'static> SyzExecHandle<E> {
    pub fn new<T: ManageVm<Error = E> + 'static>(vm_handle: T, config: SyzExecConfig) -> Self {
        let remote_syz_bin_path = PathBuf::from("~").join(config.syz_bin.file_name().unwrap());

        Self {
            syz_bin_path: config.syz_bin,
            remote_syz_bin_path,
            in_shm: None,
            out_shm: None,
            force_setup: config.force_setup,
            in_mem: None,
            out_mem: None,

            features: None,
            env_flags: 0,
            pid: 0,

            syz: None,
            syz_in_pipe: None,
            syz_out_pipe: None,
            syz_error_pipe: None,

            boot_vm: true,
            vm_handle: Box::new(vm_handle),
            vm_ip: String::new(),
            vm_port: 0,
            vm_ssh_user: String::new(),
            vm_ssh_key_path: String::new(),

            exec_count: 0,
        }
    }

    pub fn exec(
        &mut self,
        t: &Target,
        p: &Prog,
        opt: &ExecOpt,
    ) -> Result<SyzExecResult, SyzExecError> {
        const SYZ_STATUS_INTERNAL_ERROR: i32 = 67;

        if self.syz.is_none() && !stop_soon() {
            self.spawn_syz(false)?;
        }

        let use_shm = self.in_shm.is_some();
        let in_buf = self
            .in_shm
            .as_mut()
            .map(|shm| unsafe { shm.as_slice_mut() })
            .or(self.in_mem.as_deref_mut())
            .unwrap();

        let prog_sz = match serialize(t, p, in_buf) {
            Ok(left_sz) => in_buf.len() - left_sz,
            Err(e) => return Err(SyzExecError::Serialize(e)),
        };

        let exec_req = ExecuteReq {
            magic: IN_MAGIC,
            env_flags: self.env_flags,
            exec_flags: opt.flags,
            pid: self.pid,
            fault_call: opt.fault_call as u64,
            fault_nth: opt.fault_nth as u64,
            syscall_timeout_ms: 50,
            program_timeout_ms: 5000,
            slowdown_scale: 1,
            prog_size: if use_shm { 0 } else { prog_sz as u64 },
        };
        // TODO handle this error
        write_all(&mut self.syz_in_pipe.as_mut().unwrap(), &exec_req)?;
        if !use_shm {
            self.syz_in_pipe
                .as_mut()
                .unwrap()
                .write_all(&in_buf[..prog_sz])?;
        }

        let out_buf = self
            .out_shm
            .as_mut()
            .map(|shm| unsafe { shm.as_slice_mut() })
            .or(self.out_mem.as_deref_mut())
            .unwrap();

        out_buf[0..4].iter_mut().for_each(|v| *v = 0);

        let exit_status;
        let mut exec_reply: ExecuteReply;
        loop {
            exec_reply = match read_exact(&mut self.syz_out_pipe.as_mut().unwrap()) {
                Ok(r) => r,
                Err(e) => return Ok(self.handle_error(e)),
            };
            if exec_reply.magic != OUT_MAGIC {
                return Err(SyzExecError::MagicMismatch {
                    required: OUT_MAGIC,
                    got: exec_reply.magic,
                });
            }

            if exec_reply.done != 0 {
                exit_status = exec_reply.status as i32;
                break;
            }

            let _: CallReply = match read_exact(&mut self.syz_out_pipe.as_mut().unwrap()) {
                Ok(r) => r,
                Err(e) => return Ok(self.handle_error(e)),
            };
        }

        if debug() {
            log::debug!("exec replied:\n{:#?}", exec_reply);
        }

        if exit_status == SYZ_STATUS_INTERNAL_ERROR {
            // let stderr =
            // String::from_utf8_lossy(&self.syz_error_pipe.take().unwrap().wait_finish())
            // .into_owned();
            self.syz_error_pipe.take().unwrap().wait_finish();
            self.kill_syz();
            return Ok(SyzExecResult::Normal(Vec::new()));
        } else if exit_status != 0 && self.syz.as_mut().unwrap().try_wait().unwrap().is_some() {
            self.syz_error_pipe.take().unwrap().wait_finish();
            self.reset();
            return Ok(SyzExecResult::Normal(Vec::new()));
        }

        let stderr = self.syz_error_pipe.as_ref().unwrap().current_data();
        if debug() {
            let stderr_str = String::from_utf8_lossy(&stderr);
            log::debug!("exec succ:\n{}", stderr_str);
        }

        self.exec_count += 1;
        if self.exec_count % 64 == 0 {
            self.vm_handle.reset().unwrap(); // clear inner buffer
        }

        self.parse_output(p).map(SyzExecResult::Normal)
    }

    pub(super) fn parse_output(&self, p: &Prog) -> Result<Vec<CallExecInfo>, SyzExecError> {
        const EXTRA_REPLY_INDEX: u32 = 0xffffffff;

        let mut out_buf = self
            .out_shm
            .as_ref()
            .map(|shm| unsafe { shm.as_slice() })
            .or(self.out_mem.as_deref())
            .unwrap();

        let ncmd = read_u32(&mut out_buf)
            .ok_or_else(|| SyzExecError::Parse("failed to read number of calls".to_string()))?;
        let mut info = vec![CallExecInfo::default(); p.calls.len()];
        for i in 0..ncmd {
            let reply: &CallReply = read(&mut out_buf)
                .ok_or_else(|| SyzExecError::Parse(format!("failed to read call {} reply", i)))?;
            if reply.index != EXTRA_REPLY_INDEX {
                if reply.index as usize > info.len() {
                    return Err(SyzExecError::Parse(format!(
                        "bad call {} index {}/{}",
                        i,
                        reply.index,
                        info.len()
                    )));
                }
                let sid = p.calls[reply.index as usize].meta.id;
                if sid != reply.num as usize {
                    return Err(SyzExecError::Parse(format!(
                        "wrong call {} num {}/{}",
                        i, reply.num, sid
                    )));
                }
                let call_info = &mut info[reply.index as usize];
                if call_info.flags != 0 || !call_info.branches.is_empty() {
                    return Err(SyzExecError::Parse(format!(
                        "duplicate reply for call {}/{}/{}",
                        i, reply.index, reply.num
                    )));
                }

                if reply.comps_size != 0 {
                    return Err(SyzExecError::Parse(format!(
                        "comparison collected for call {}/{}/{}",
                        i, reply.index, reply.num
                    )));
                }
                call_info.flags = reply.flags;
                call_info.errno = reply.errno as i32;
                if reply.branch_size != 0 {
                    let br = read_u32_slice(&mut out_buf, reply.branch_size as usize).ok_or_else(
                        || {
                            SyzExecError::Parse(format!(
                                "call {}/{}/{}: signal overflow: {}/{}",
                                i,
                                reply.index,
                                reply.num,
                                reply.branch_size,
                                out_buf.len()
                            ))
                        },
                    )?;
                    // TODO do not copy.
                    call_info.branches.extend(br);
                }
                if reply.block_size != 0 {
                    let bk = read_u32_slice(&mut out_buf, reply.block_size as usize).ok_or_else(
                        || {
                            SyzExecError::Parse(format!(
                                "call {}/{}/{}: cover overflow: {}/{}",
                                i,
                                reply.index,
                                reply.num,
                                reply.block_size,
                                out_buf.len()
                            ))
                        },
                    )?;
                    // TODO do not copy.
                    call_info.blocks.extend(bk);
                }
            }
        }
        Ok(info)
    }

    pub fn spawn_syz(&mut self, force_reboot: bool) -> Result<(), SyzSpawnError> {
        if self.boot_vm || force_reboot {
            if debug() {
                log::debug!("vm handle not alive, booting...");
            }
            self.setup()?;
            self.boot_vm = false;
        }
        self.do_spawn(true)
    }

    fn setup(&mut self) -> Result<(), SyzSpawnError> {
        self.vm_handle
            .boot()
            .map_err(|e| SyzSpawnError::VmHandleBoot(Box::new(e)))?;

        let (ip, port) = self.vm_handle.addr().unwrap();
        let (ssh_key, ssh_user) = self.vm_handle.ssh().unwrap();
        // update after every boot
        self.vm_ip = ip;
        self.vm_port = port;
        self.vm_ssh_key_path = ssh_key.to_str().unwrap().to_string();
        self.vm_ssh_user = ssh_user;
        log::debug!(
            "vm ip: {}, port: {}, key: {}, user: {}",
            self.vm_ip,
            self.vm_port,
            self.vm_ssh_key_path,
            self.vm_ssh_user
        );

        if let Err(e) = self.do_setup() {
            if self.force_setup {
                return Err(e);
            } else {
                log::warn!("failed to setup syz-executor: {}", e);
            }
        }
        Ok(())
    }

    fn syz_cmd(&self) -> Command {
        let mut syz = self.ssh_basic_cmd();
        syz.arg(&self.remote_syz_bin_path)
            .arg("use-ivshm")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        syz
    }

    fn do_spawn(&mut self, mut force_reboot: bool) -> Result<(), SyzSpawnError> {
        let mut syz = self.syz_cmd();
        if debug() {
            log::debug!("spawning command:\n{:?}", syz);
        }
        let mut tries = 0;
        loop {
            let mut child = syz.spawn()?;
            let stdin = child.stdin.take().unwrap();
            let stdout = child.stdout.take().unwrap();
            let stderr = read_background(child.stderr.take().unwrap());
            self.syz = Some(child);
            self.syz_in_pipe = Some(stdin);
            self.syz_out_pipe = Some(stdout);
            self.syz_error_pipe = Some(stderr);
            let ret = self.handshake();
            if let Err(ref e) = ret {
                self.kill_syz();
                if tries != 3 {
                    tries += 1;
                    log::warn!("failed to spawn syz: {}", e);
                    log::warn!("retrying ({}) ...", tries);
                    continue;
                } else if force_reboot {
                    log::warn!("failed to spawn syz, force rebooting...");
                    self.setup()?;
                    syz = self.syz_cmd();
                    force_reboot = false;
                    continue;
                }
            }
            break ret;
        }
    }

    fn handshake(&mut self) -> Result<(), SyzSpawnError> {
        let req = HandshakeReq {
            magic: IN_MAGIC,
            env_flags: self.env_flags,
            pid: self.pid,
        };
        write_all(&mut self.syz_in_pipe.as_mut().unwrap(), &req).map_err(|e| {
            SyzSpawnError::HandShake(format!("failed to write handshake req: {}", e))
        })?;
        if debug() {
            log::debug!("handshake req sent:\n{:#?}", req);
        }

        let reply: HandshakeReply = match read_exact(&mut self.syz_out_pipe.as_mut().unwrap()) {
            Ok(r) => r,
            Err(e) => {
                let stderr = self.syz_error_pipe.take().unwrap().wait_finish();
                let stderr = String::from_utf8_lossy(&stderr);
                let e = SyzSpawnError::HandShake(format!(
                    "failed to read handshake reply: {}, syz stderr: {}",
                    e,
                    stderr.trim()
                ));
                return Err(e);
            }
        };

        if reply.magic != OUT_MAGIC {
            Err(SyzSpawnError::HandShake(format!(
                "reply magic not match, require: {:x}, received: {:x}",
                OUT_MAGIC, reply.magic
            )))
        } else {
            if debug() {
                log::debug!("handshake succ:\n{:#?}", reply);
            }
            Ok(())
        }
    }

    fn do_setup(&mut self) -> Result<(), SyzSpawnError> {
        scp(
            &self.vm_ip,
            self.vm_port,
            &self.vm_ssh_key_path,
            &self.vm_ssh_user,
            &self.syz_bin_path,
            &self.remote_syz_bin_path,
        )
        .map_err(|e| {
            SyzSpawnError::Setup(format!(
                "failed to scp, from: {}, to: {}, error: {}",
                self.syz_bin_path.display(),
                self.remote_syz_bin_path.display(),
                e
            ))
        })?;
        if debug() {
            log::debug!(
                "scp succ: from '{}', to '{}'",
                &self.syz_bin_path.display(),
                &self.remote_syz_bin_path.display()
            );
        }

        if self.features.is_none() {
            let mut syz_check = self.ssh_basic_cmd();
            syz_check.arg(&self.remote_syz_bin_path).arg("check");
            let output = syz_check.output().map_err(|e| {
                SyzSpawnError::Setup(format!("failed to run '{:?}': {}", syz_check, e))
            })?;
            if output.status.success() {
                let out = output.stdout;
                assert_eq!(out.len(), 8);
                let mut val = [0; 8];
                val.copy_from_slice(&out[0..]);
                self.features = Some(u64::from_le_bytes(val));
                self.env_flags = features_to_env_flags(self.features.unwrap());
                if debug() {
                    self.env_flags |= FLAG_DEBUG;
                }
            } else {
                let err = String::from_utf8_lossy(&output.stderr).into_owned();
                return Err(SyzSpawnError::Setup(format!(
                    "failed to run '{:?}' : {}",
                    syz_check, err
                )));
            }
        }

        let mut syz_setup = self.ssh_basic_cmd();
        syz_setup.arg(&self.remote_syz_bin_path).arg("setup");
        let features = extract_setup_args(self.features.unwrap());
        syz_setup.args(&features);
        let output = syz_setup
            .output()
            .map_err(|e| SyzSpawnError::Setup(format!("failed to run '{:?}': {}", syz_setup, e)))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(SyzSpawnError::Setup(format!(
                "failed to run '{:?}': {}",
                syz_setup, err
            )));
        }
        if debug() {
            log::debug!("syz setup succ: command:\n{:?}", syz_setup);
        }
        Ok(())
    }

    // fn send_exec_request(&mut self, data: &[u8]) -> Result<u32, RequestError> {
    //     if let Some(in_shm) = self.in_shm.as_mut() {
    //         unsafe {
    //             std::ptr::copy(data.as_ptr(), in_shm.as_ptr(), data.len());
    //         }
    //     }

    //     let exec_req = ExecuteReq {
    //         magic: IN_MAGIC,
    //         env_flags: self.env_flags,
    //         exec_flags: self.exec_opts.flags,
    //         pid: self.pid,
    //         fault_call: self.exec_opts.fault_call as u64,
    //         fault_nth: self.exec_opts.fault_nth as u64,
    //         syscall_timeout_ms: 50,
    //         program_timeout_ms: 5000,
    //         slowdown_scale: 1,
    //         prog_size: if self.in_shm.is_some() {
    //             0
    //         } else {
    //             data.len() as u64
    //         },
    //     };
    //     write_all(&mut self.syz_in_pipe.as_mut().unwrap(), &exec_req)?;
    //     if self.in_shm.is_none() {
    //         self.syz_in_pipe.as_mut().unwrap().write_all(data)?;
    //     }

    //     let pid = self.syz.as_ref().unwrap().id() as u32;
    //     if debug() {
    //         log::debug!("exec req sent to syz (pid {}):\n {:#?}", pid, exec_req);
    //     }
    //     Ok(pid)
    // }

    // fn wait_exec_response(&mut self) -> Result<super::ExecResult, WaitError> {
    //     const SYZ_FAIL_STATUS: i32 = 67;
    //     let exit_status;
    //     let mut exec_reply: ExecuteReply;
    //     loop {
    //         exec_reply = match read_exact(&mut self.syz_out_pipe.as_mut().unwrap()) {
    //             Ok(r) => r,
    //             Err(e) => return Ok(self.handle_error(e)),
    //         };
    //         if exec_reply.magic != OUT_MAGIC {
    //             return Err(WaitError::HandShake(format!(
    //                 "exec magic number not match, expected: {}, recved: {}",
    //                 OUT_MAGIC, exec_reply.magic
    //             )));
    //         }
    //         if exec_reply.done != 0 {
    //             exit_status = exec_reply.status as i32;
    //             break;
    //         }
    //         let _: CallReply = match read_exact(&mut self.syz_out_pipe.as_mut().unwrap()) {
    //             Ok(r) => r,
    //             Err(e) => return Ok(self.handle_error(e)),
    //         };
    //     }

    //     if debug() {
    //         log::debug!("exec replied:\n{:#?}", exec_reply);
    //     }

    //     if exit_status == SYZ_FAIL_STATUS {
    //         let stderr =
    //             String::from_utf8_lossy(&self.syz_error_pipe.take().unwrap().wait_finish())
    //                 .into_owned();
    //         self.kill_syz();

    //         return Ok(SyzExecResult::Normal(Vec::new()));
    //     } else if exit_status != 0 {
    //         if let Some(status) = self.syz.as_mut().unwrap().try_wait().unwrap() {
    //             let stderr =
    //                 String::from_utf8_lossy(&self.syz_error_pipe.take().unwrap().wait_finish())
    //                     .into_owned();
    //             self.reset();
    //             return Ok(SyzExecResult::Normal(Vec::new()));
    //         }
    //     }

    //     let stderr = self.syz_error_pipe.as_ref().unwrap().current_data();
    //     if debug() {
    //         let stderr_str = String::from_utf8_lossy(&stderr);
    //         log::debug!("exec succ:\n{}", stderr_str);
    //     }
    //     self.exec_count += 1;
    //     if self.exec_count % 64 == 0 {
    //         self.vm_handle.reset().unwrap(); // clear inner buffer
    //     }
    //     Ok(ExecResult::Finished)
    // }

    fn handle_error(&mut self, _: std::io::Error) -> SyzExecResult {
        // use std::os::unix::process::ExitStatusExt;

        let status = self.syz.as_mut().unwrap().wait().unwrap();
        let stderr = self.syz_error_pipe.take().unwrap().wait_finish();
        if debug() {
            let stderr = String::from_utf8_lossy(&stderr);
            log::debug!("pipe broken: status: {:?}, msg: {}", status, stderr);
        }

        self.reset();
        // if let Some(code) = status.signal() {
        //     if code == nix::sys::signal::SIGKILL as i32 {
        //         self.kill_remote_syz();
        //         if self.do_spawn(false).is_err() {
        //             let log = self.vm_handle.collect_crash_log();
        //             self.boot_vm = true;
        //             ExecResult::KernelCrash { log }
        //         } else {
        //             ExecResult::Hang { code }
        //         }
        //     } else {
        //         let stderr = String::from_utf8_lossy(&stderr).into_owned();
        //         ExecResult::ExecExited {
        //             code: status.code().unwrap(),
        //             msg: stderr,
        //         }
        //     }
        // } else

        if !self.vm_handle.is_alive() {
            let log = self.vm_handle.collect_crash_log();
            self.boot_vm = true;
            SyzExecResult::Crash(log)
            // ExecResult::KernelCrash { log }
        } else {
            self.kill_remote_syz();
            // let stderr = String::from_utf8_lossy(&stderr).into_owned();
            SyzExecResult::Normal(Vec::new())
            // ExecResult::ExecExited {
            // code: status.code().unwrap(),
            // msg: stderr,
            // }
        }
    }

    fn kill_remote_syz(&mut self) {
        let mut pkill = self.ssh_basic_cmd();
        let _ = pkill
            .arg("pkill")
            .arg(self.remote_syz_bin_path.file_name().unwrap())
            .output()
            .unwrap();
    }

    fn kill_syz(&mut self) {
        if let Some(syz) = self.syz.as_mut() {
            let _ = syz.kill();
            let _ = syz.wait();
        }
        self.reset();
    }

    fn reset(&mut self) {
        self.syz = None;
        self.syz_error_pipe = None;
        self.syz_in_pipe = None;
        self.syz_out_pipe = None;
    }

    #[inline]
    fn ssh_basic_cmd(&self) -> Command {
        ssh_basic_cmd(
            &self.vm_ip,
            self.vm_port,
            &self.vm_ssh_key_path,
            &self.vm_ssh_user,
        )
    }
}

impl<E: std::error::Error> Drop for SyzExecHandle<E> {
    fn drop(&mut self) {
        if let Some(qemu) = self.syz.as_mut() {
            let _ = qemu.kill();
            let _ = qemu.wait();
        }
    }
}

// impl Exec for SyzExecHandle {
//     type RequestError = RequestError;
//     type WaitError = WaitError;

//     fn request_exec(&mut self, data: &[u8]) -> Result<u32, Self::RequestError> {
//         if self.syz.is_none() {
//             self.spawn_syz()?;
//         }
//         if data.len() < INPUT_SHM_SZ {
//             let ret = self.send_exec_request(data);
//             if ret.is_ok() {
//                 self.request_sent = true;
//             }
//             ret
//         } else {
//             Ok(self.syz.as_ref().unwrap().id())
//         }
//     }

//     fn wait_exec_finish(&mut self) -> Result<ExecResult, Self::WaitError> {
//         if self.request_sent {
//             self.request_sent = false;
//             self.wait_exec_response()
//         } else {
//             Ok(ExecResult::Finished)
//         }
//     }

//     fn shms(&mut self) -> Option<(&mut [u8], &mut [u8])> {
//         if let (Some(in_shm), Some(out_shm)) = (self.in_shm.as_mut(), self.out_shm.as_mut()) {
//             Some(unsafe { (in_shm.as_slice_mut(), out_shm.as_slice_mut()) })
//         } else {
//             None
//         }
//     }
// }

#[derive(Debug, Error)]
pub enum RequestError {
    #[error("failed to spawn executor: {0}")]
    Spawn(#[from] SyzSpawnError),
    #[error("io error during requesting: {0}")]
    IO(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum WaitError {
    #[error("handshake: {0}")]
    HandShake(String),
    #[error("io error during waiting: {0}")]
    IO(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum SyzSpawnError {
    #[error("vm handle: {0}")]
    VmHandleBoot(Box<dyn std::error::Error + 'static>),
    #[error("setup: {0}")]
    Setup(String),
    #[error("spawn: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("handshake: {0}")]
    HandShake(String),
}

#[derive(Debug, Clone)]
pub struct SyzExecInfo {
    pub os: String,
    pub arch: String,
    pub syz_revision: String,
    pub syzlang_revision: String,
}

impl Display for SyzExecInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "os {}, arch {}, syzlang {}, syz {}",
            self.os,
            self.arch,
            &self.syzlang_revision[0..12],
            &self.syz_revision[0..12]
        )
    }
}

#[derive(Error, Debug)]
pub enum SyzExecInfoDetectError {
    #[error("spawn: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(String),
    #[error("invalid syz executor info: {0}")]
    Invalidate(String),
    #[error("execution: {0}")]
    Execution(String),
}

impl SyzExecInfo {
    pub fn detect<T: AsRef<Path>>(syz_bin: T) -> Result<SyzExecInfo, SyzExecInfoDetectError> {
        let syz_bin = syz_bin.as_ref();
        let output = Command::new(syz_bin).arg("version").output()?;
        if output.status.success() {
            let output = String::from_utf8_lossy(&output.stdout).into_owned();
            let syz_info = output.trim().split_ascii_whitespace().collect::<Vec<_>>();
            if syz_info.len() != 4 {
                Err(SyzExecInfoDetectError::Parse(format!(
                    "unsupported format: {}",
                    output
                )))
            } else {
                let info = SyzExecInfo {
                    os: syz_info[0].to_string(),
                    arch: syz_info[1].to_string(),
                    syz_revision: syz_info[3].to_string(),
                    syzlang_revision: syz_info[2].to_string(),
                };
                if !info.validate() {
                    Err(SyzExecInfoDetectError::Invalidate(output))
                } else {
                    Ok(info)
                }
            }
        } else {
            Err(SyzExecInfoDetectError::Execution(format!(
                "failed to execute '{} version': {}",
                syz_bin.display(),
                output.status
            )))
        }
    }

    pub fn validate(&self) -> bool {
        const ARCHES: [&str; 8] = [
            "arm", "arm64", "386", "amd64", "mips64le", "ppc64le", "riscv64", "s390x",
        ];
        ARCHES.contains(&&self.arch[..])
    }
}

/// Possible result of one execution.
pub enum ExecResult {
    /// Prog was executed successfully without crashing kernel or executor.
    Normal(Vec<CallExecInfo>),
    /// Prog was executed partially(executor hang or exited) without crashing kernel.
    Failed {
        info: Vec<CallExecInfo>,
        err: Box<dyn std::error::Error + 'static>,
    },
    /// Prog caused kernel panic.
    Crash(Vec<u8>),
}

/// Flag for execution result of one call.
pub type CallFlags = u32;

iota! {
    pub const CALL_EXECUTED : CallFlags = 1 << (iota); // was started at all
    , CALL_FINISHED                                // finished executing (rather than blocked forever)
    , CALL_BLOCKED                                 // finished but blocked during execution
    , CALL_FAULT_INJECTED                          // fault was injected into this call
}

/// Execution of one call.
#[derive(Debug, Default, Clone)]
pub struct CallExecInfo {
    pub flags: CallFlags,
    /// Branch coverage.
    pub branches: Vec<u32>,
    /// Block converage.
    pub blocks: Vec<u32>,
    /// Syscall errno, indicating the success or failure.
    pub errno: i32,
}

/// Env flags to executor.
type EnvFlags = u64;

iota! {
    const FLAG_DEBUG: EnvFlags = 1 << (iota);             // debug output from executor
    , FLAG_SIGNAL                                    // collect feedback signals (coverage)
    , FLAG_SANDBOX_SETUID                            // impersonate nobody user
    , FLAG_SANDBOX_NAMESPACE                         // use namespaces for sandboxing
    , FLAG_SANDBOX_ANDROID                           // use Android sandboxing for the untrusted_app domain
    , FLAG_EXTRA_COVER                               // collect extra coverage
    , FLAG_ENABLE_TUN                                // setup and use /dev/tun for packet injection
    , FLAG_ENABLE_NETDEV                             // setup more network devices for testing
    , FLAG_ENABLE_NETRESET                           // reset network namespace between programs
    , FLAG_ENABLE_CGROUPS                            // setup cgroups for testing
    , FLAG_ENABLE_CLOSEFDS                           // close fds after each program
    , FLAG_ENABLE_DEVLINKPCI                         // setup devlink PCI device
    , FLAG_ENABLE_VHCI_INJECTION                     // setup and use /dev/vhci for hci packet injection
    , FLAG_ENABLE_WIFI                               // setup and use mac80211_hwsim for wifi emulation
}

fn features_to_env_flags(features: u64) -> EnvFlags {
    let mut env = FLAG_SIGNAL;

    if features & FEATURE_EXTRA_COVERAGE != 0 {
        env |= FLAG_EXTRA_COVER;
    }
    if features & FEATURE_NET_INJECTION != 0 {
        env |= FLAG_ENABLE_TUN;
    }
    if features & FEATURE_NET_DEVICES != 0 {
        env |= FLAG_ENABLE_NETDEV;
    }

    env |= FLAG_ENABLE_NETRESET;
    env |= FLAG_ENABLE_CGROUPS;
    env |= FLAG_ENABLE_CLOSEFDS;

    if features & FEATURE_DEVLINK_PCI != 0 {
        env |= FLAG_ENABLE_DEVLINKPCI;
    }
    if features & FEATURE_VHCI_INJECTION != 0 {
        env |= FLAG_ENABLE_VHCI_INJECTION;
    }
    if features & FEATURE_WIFI_EMULATION != 0 {
        env |= FLAG_ENABLE_WIFI;
    }

    env
}

iota! {
    pub const FEATURE_COVERAGE: u64 = 1 << (iota);
    ,FEATURE_COMPARISONS
    ,FEATURE_EXTRA_COVERAGE
    ,FEATURE_SANDBOX_SETUID
    ,FEATURE_SANDBOX_NAMESPACE
    ,FEATURE_SANDBOX_ANDROID
    ,FEATURE_FAULT
    ,FEATURE_LEAK
    ,FEATURE_NET_INJECTION
    ,FEATURE_NET_DEVICES
    ,FEATURE_KCSAN
    ,FEATURE_DEVLINK_PCI
    ,FEATURE_USB_EMULATION
    ,FEATURE_VHCI_INJECTION
    ,FEATURE_WIFI_EMULATION
    ,FEATURE_802154
}

fn extract_setup_args(features: u64) -> Vec<String> {
    let mut ret = Vec::new();

    if features & FEATURE_LEAK != 0 {
        ret.push("leak".to_string());
    }
    if features & FEATURE_FAULT != 0 {
        ret.push("fault".to_string());
    }
    if features & FEATURE_KCSAN != 0 {
        ret.push("kcsan".to_string());
    }
    if features & FEATURE_USB_EMULATION != 0 {
        ret.push("usb".to_string());
    }
    if features & FEATURE_802154 != 0 {
        ret.push("802154".to_string());
    }

    ret
}

const IN_MAGIC: u64 = 0xBADC0FFEEBADFACE;
const OUT_MAGIC: u32 = 0xBADF00D;

#[repr(C)]
#[derive(Default, Debug)]
struct HandshakeReq {
    magic: u64,
    env_flags: u64, // env flags
    pid: u64,
}
#[repr(C)]
#[derive(Default, Debug)]
struct HandshakeReply {
    magic: u32,
}

#[repr(C)]
#[derive(Default, Debug)]
struct ExecuteReq {
    magic: u64,
    env_flags: u64,  // env flags
    exec_flags: u64, // exec flags
    pid: u64,
    fault_call: u64,
    fault_nth: u64,
    syscall_timeout_ms: u64,
    program_timeout_ms: u64,
    slowdown_scale: u64,
    prog_size: u64,
}

#[repr(C)]
#[derive(Default, Debug)]
struct ExecuteReply {
    magic: u32,
    done: u32,
    status: u32,
}

#[repr(C)]
#[derive(Default, Debug)]
struct CallReply {
    index: u32, // call index in the program
    num: u32,   // syscall number (for cross-checking)
    errno: u32,
    flags: u32, // see CallFlags
    branch_size: u32,
    block_size: u32,
    comps_size: u32,
}

#[derive(Debug, Error)]
pub enum ShmSetUpError {
    #[error("syz-executor does not support shm under target '{0}'")]
    TargetNotSupported(String),
    #[error("failed to open output shm '{id}': {error}")]
    OpenShm {
        id: String,
        error: shared_memory::ShmemError,
    },
    #[error("failed to create shm '{id}': {error}")]
    CreateShm {
        id: String,
        error: shared_memory::ShmemError,
    },
    #[error("TODO")]
    BadOutShm { afl: String, manual: String },
    #[error("io: {0}")]
    IO(std::io::Error),
}

pub fn setup_shm(
    target: &str,
    manual_shms: Option<(String, String)>,
) -> Result<Option<(shared_memory::Shmem, shared_memory::Shmem)>, ShmSetUpError> {
    const INPUT_SHM_SZ: usize = 4 << 20;
    const OUTPUT_SHM_SZ: usize = 8 << 20;

    if target_requires_shm(target) {
        if let Some((in_shmid, out_shmid)) = manual_shms {
            let out_shm =
                ShmemConf::new()
                    .os_id(&out_shmid)
                    .open()
                    .map_err(|e| ShmSetUpError::OpenShm {
                        id: out_shmid,
                        error: e,
                    })?;
            let in_shm =
                ShmemConf::new()
                    .os_id(&in_shmid)
                    .open()
                    .map_err(|e| ShmSetUpError::OpenShm {
                        id: in_shmid,
                        error: e,
                    })?;
            Ok(Some((in_shm, out_shm)))
        } else {
            let out_shmid = format!("grafter-output-shm-{}", std::process::id());
            let mut out_shm = ShmemConf::new()
                .os_id(&out_shmid)
                .size(OUTPUT_SHM_SZ)
                .create()
                .map_err(|e| ShmSetUpError::CreateShm {
                    id: out_shmid,
                    error: e,
                })?;
            out_shm.set_owner(true);

            let in_shmid = format!("grafter-input-shm-{}", std::process::id());
            let mut in_shm = ShmemConf::new()
                .os_id(&in_shmid)
                .size(INPUT_SHM_SZ)
                .create()
                .map_err(|e| ShmSetUpError::CreateShm {
                    id: in_shmid,
                    error: e,
                })?;
            in_shm.set_owner(true);
            Ok(Some((in_shm, out_shm)))
        }
    } else if manual_shms.is_some() {
        Err(ShmSetUpError::TargetNotSupported(target.to_string()))
    } else {
        Ok(None)
    }
}

fn target_requires_shm(target: &str) -> bool {
    const REQUIRED_TARGETS: [&str; 12] = [
        "freebsd/386",
        "freebsd/amd64",
        "linux/386",
        "linux/amd64",
        "linux/arm",
        "linux/arm64",
        "linux/mips64le",
        "linux/ppc64le",
        "linux/riscv64",
        "linux/s390",
        "netbsd/amd64",
        "openbsd/amd64",
    ];
    REQUIRED_TARGETS.contains(&target)
}

/// Flag for controlling execution behavior.
pub type ExecFlags = u64;

iota! {
    pub const FLAG_COLLECT_COVER : ExecFlags = 1 << (iota);       // collect coverage
    , FLAG_DEDUP_COVER                                 // deduplicate coverage in executor
    , FLAG_INJECT_FAULT                                // inject a fault in this execution (see ExecOpts)
    , FLAG_COLLECT_COMPS                               // collect KCOV comparisons
    , FLAG_THREADED                                    // use multiple threads to mitigate blocked syscalls
    , FLAG_COLLIDE                                     // collide syscalls to provoke data races
    , FLAG_ENABLE_COVERAGE_FILTER                      // setup and use bitmap to do coverage filter
}

/// Option for controlling execution behavior.
#[derive(Debug, Clone, Default)]
pub struct ExecOpt {
    /// Options for this execution.
    pub flags: ExecFlags,
    /// Inject fault for 'fault_call'.
    pub fault_call: i32,
    /// Inject fault 'nth' for 'fault_call'
    pub fault_nth: i32,
}

impl ExecOpt {
    pub const fn new() -> Self {
        Self {
            flags: FLAG_DEDUP_COVER | FLAG_THREADED | FLAG_COLLIDE,
            fault_call: 0,
            fault_nth: 0,
        }
    }

    pub const fn new_no_collide() -> Self {
        Self {
            flags: FLAG_DEDUP_COVER | FLAG_THREADED,
            fault_call: 0,
            fault_nth: 0,
        }
    }

    pub const fn new_cover() -> Self {
        Self {
            flags: FLAG_DEDUP_COVER | FLAG_THREADED | FLAG_COLLECT_COVER,
            fault_call: 0,
            fault_nth: 0,
        }
    }
}
