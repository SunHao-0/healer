//! Start, interactive with syz-executor

use hlang::ast::Prog;
use std::{
    error::Error,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
};
use thiserror::Error;

use super::{
    serialize::serialize,
    ssh::{scp, ssh_basic_cmd, ScpError},
    CallExecInfo, EnvFlags, ExecOpt,
};
use crate::{bg_task::Reader, target::Target};

#[derive(Debug, Error)]
pub enum SyzSpawnError {
    #[error("config: {0}")]
    Config(String),
    #[error("spawn: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("copy syz-executor: {0}")]
    Scp(#[from] ScpError),
    #[error("waiting handshake: {0}")]
    HandShake(String),
}

pub(super) struct SyzHandleBuilder {
    ssh_key: Option<String>,
    ssh_user: Option<String>,
    ssh_ip: Option<String>,
    ssh_port: Option<u16>,
    executor: Option<Box<Path>>,
    env_flags: EnvFlags,
    use_forksrv: bool,
    use_shm: bool,
    copy_bin: bool,
    extra_args: Vec<String>,
    scp_path: Option<Box<Path>>,
    pid: Option<u64>,
}

impl Default for SyzHandleBuilder {
    fn default() -> SyzHandleBuilder {
        SyzHandleBuilder::new()
    }
}

impl SyzHandleBuilder {
    pub(super) fn new() -> Self {
        Self {
            ssh_key: None,
            ssh_user: None,
            ssh_ip: None,
            ssh_port: None,
            executor: None,
            env_flags: super::FLAG_SIGNAL,
            use_forksrv: true,
            use_shm: true,
            copy_bin: true,
            extra_args: Vec::new(),
            scp_path: None,
            pid: None,
        }
    }

    pub(super) fn ssh_addr<T: Into<String>>(mut self, ip: T, port: u16) -> Self {
        self.ssh_ip = Some(ip.into());
        self.ssh_port = Some(port);
        self
    }

    pub(super) fn ssh_identity<T: Into<String>>(mut self, key: T, user: T) -> Self {
        self.ssh_user = Some(user.into());
        self.ssh_key = Some(key.into());
        self
    }

    pub(super) fn env_flags(mut self, flag: u64) -> Self {
        self.env_flags = flag;
        self
    }

    pub(super) fn executor(mut self, p: Box<Path>) -> Self {
        self.executor = Some(p);
        self
    }

    pub(super) fn use_forksrv(mut self, u: bool) -> Self {
        self.use_forksrv = u;
        self
    }

    pub(super) fn user_shm(mut self, u: bool) -> Self {
        self.use_shm = true;
        self
    }

    pub(super) fn copy_bin(mut self, u: bool) -> Self {
        self.copy_bin = u;
        self
    }

    pub(super) fn pid(mut self, pid: u64) -> Self {
        self.pid = Some(pid);
        self
    }

    pub(super) fn extra_arg<T: Into<String>>(mut self, arg: T) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    pub(super) fn extra_args<T: IntoIterator<Item = F>, F: Into<String>>(
        mut self,
        args: T,
    ) -> Self {
        self.extra_args
            .extend(args.into_iter().map(|arg| arg.into()));
        self
    }

    pub(super) fn scp_path(mut self, p: Box<Path>) -> Self {
        self.scp_path = Some(p);
        self
    }

    pub(super) fn spawn(self) -> Result<SyzHandle, SyzSpawnError> {
        let pid = self
            .pid
            .ok_or_else(|| SyzSpawnError::Config("need pid".to_string()))?;
        let mut syz = self.spawn_remote()?;
        let stdin = syz.stdin.take().unwrap();
        let stdout = syz.stdout.take().unwrap();
        let stderr = Reader::new(syz.stderr.take().unwrap());
        let mut syz_handle = SyzHandle {
            syz,
            stdin,
            stdout,
            pid,
            bg_stderr: stderr,
            use_shm: self.use_shm,
            env_flags: self.env_flags,
        };

        if self.use_forksrv {
            if let Err(e) = syz_handle.handshake() {
                let stderr = String::from_utf8(syz_handle.output()).unwrap_or_default();
                return Err(SyzSpawnError::HandShake(format!(
                    "{}\nSTDERR:\n{}",
                    e, stderr
                )));
            }
        } else {
            // use fork server by default for now.
            todo!()
        }
        Ok(syz_handle)
    }

    fn spawn_remote(&self) -> Result<Child, SyzSpawnError> {
        let from = self
            .executor
            .as_deref()
            .ok_or_else(|| SyzSpawnError::Config("need executable file".to_string()))?;
        let bin = from.file_name().ok_or_else(|| {
            SyzSpawnError::Config(format!("bad executable file path: {}", from.display()))
        })?;
        let mut to = if let Some(p) = self.scp_path.as_ref() {
            p.to_path_buf()
        } else {
            PathBuf::from("~")
        };
        to.push(&bin);

        let ssh_ip = self
            .ssh_ip
            .as_deref()
            .ok_or_else(|| SyzSpawnError::Config("need ssh ip".to_string()))?;
        let ssh_port = self
            .ssh_port
            .ok_or_else(|| SyzSpawnError::Config("need ssh port".to_string()))?;
        let ssh_key = self
            .ssh_key
            .as_ref()
            .ok_or_else(|| SyzSpawnError::Config("need key".to_string()))?;
        let ssh_user = self.ssh_user.as_deref().unwrap_or("root");

        if self.copy_bin {
            scp(ssh_ip, ssh_port, ssh_key, ssh_user, from, &to)?;
        }

        let mut ssh_cmd = ssh_basic_cmd(ssh_ip, ssh_port, ssh_key, ssh_user);
        ssh_cmd
            .arg(to)
            .args(&self.extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(From::from)
    }
}

pub struct SyzHandle {
    pub(crate) syz: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: ChildStdout,
    pub(crate) bg_stderr: Reader,
    pub(crate) pid: u64,
    pub(crate) use_shm: bool,
    pub(crate) env_flags: EnvFlags,
}

pub(super) enum SyzExecResult {
    Ok(Vec<CallExecInfo>),
    Failed {
        info: Vec<CallExecInfo>,
        err: Box<dyn Error + 'static>,
    },
    Internal(Box<dyn Error + 'static>),
}

impl SyzHandle {
    pub(super) fn exec(
        &mut self,
        t: &Target,
        p: &Prog,
        opt: ExecOpt,
        in_buf: &mut [u8],
        out_buf: &mut [u8],
    ) -> SyzExecResult {
        const SYZ_STATUS_INTERNAL_ERROR: i32 = 67;

        let prog_sz = match serialize(t, p, in_buf) {
            Ok(left_sz) => in_buf.len() - left_sz,
            Err(e) => return SyzExecResult::Internal(Box::new(e)),
        };

        let mut failed = false;
        let mut err = String::new();
        out_buf[0..4].iter_mut().for_each(|v| *v = 0);
        if let Err(e) = self.exec_inner(opt, &in_buf[0..prog_sz], out_buf) {
            let exit_status = match self.syz.kill() {
                Ok(_) => self.syz.wait().unwrap().code().unwrap_or(-1),
                Err(e) if e.kind() == ErrorKind::InvalidInput => {
                    self.syz.wait().unwrap().code().unwrap_or(-1)
                }
                Err(e) => panic!("unexpected error {}", e),
            };
            let stderr = self.bg_stderr.recv.recv().unwrap();
            let std_err_str = String::from_utf8(stderr).unwrap_or_default();
            if exit_status == SYZ_STATUS_INTERNAL_ERROR {
                return SyzExecResult::Internal(std_err_str.into());
            } else {
                err = format!("exec error: {}\nsyz stderr: {}", e, std_err_str);
            }
            failed = true;
        }
        match self.parse_output(p, out_buf) {
            Ok(info) => {
                if failed {
                    SyzExecResult::Failed {
                        info,
                        err: err.into(),
                    }
                } else {
                    SyzExecResult::Ok(info)
                }
            }
            Err(e) => SyzExecResult::Internal(Box::new(e)),
        }
    }

    fn output(mut self) -> Vec<u8> {
        self.kill();
        self.bg_stderr.recv.recv().unwrap()
    }

    fn kill(&mut self) {
        if self.syz.kill().is_ok() {
            let _ = self.syz.wait();
        }
    }
}

impl Drop for SyzHandle {
    fn drop(&mut self) {
        self.kill();
    }
}
