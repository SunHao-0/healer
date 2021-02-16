//! Start, interactive with syz-executor
use crate::{
    exec::{
        serialize::serialize,
        ssh::{scp, ssh_basic_cmd, ScpError},
        CallExecInfo, EnvFlags, ExecOpt,
    },
    fuzz::features,
};
use crate::{model::Prog, utils::into_async_file};
use crate::{targets::Target, utils::LogReader};

use std::{
    error::Error,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyzError {
    #[error("setup: {0}")]
    Setup(String),
    #[error("check features: {0}")]
    CheckFeatures(String),
    #[error("config: {0}")]
    Config(String),
    #[error("spawn: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("copy syz-executor: {0}")]
    Scp(#[from] ScpError),
    #[error("waiting handshake: {0}")]
    HandShake(String),
}

pub struct SyzHandleBuilder {
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
    pub fn new() -> Self {
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

    pub fn ssh_addr<T: Into<String>>(mut self, ip: T, port: u16) -> Self {
        self.ssh_ip = Some(ip.into());
        self.ssh_port = Some(port);
        self
    }

    pub fn ssh_identity<T: Into<String>>(mut self, key: T, user: T) -> Self {
        self.ssh_user = Some(user.into());
        self.ssh_key = Some(key.into());
        self
    }

    pub fn env_flags(mut self, flag: u64) -> Self {
        self.env_flags = flag;
        self
    }

    pub fn executor(mut self, p: Box<Path>) -> Self {
        self.executor = Some(p);
        self
    }

    pub fn use_forksrv(mut self, u: bool) -> Self {
        self.use_forksrv = u;
        self
    }

    pub fn use_shm(mut self, u: bool) -> Self {
        self.use_shm = u;
        self
    }

    pub fn copy_bin(mut self, u: bool) -> Self {
        self.copy_bin = u;
        self
    }

    pub fn pid(mut self, pid: u64) -> Self {
        self.pid = Some(pid);
        self
    }

    pub fn extra_arg<T: Into<String>>(mut self, arg: T) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    pub fn extra_args<T: IntoIterator<Item = F>, F: Into<String>>(mut self, args: T) -> Self {
        self.extra_args
            .extend(args.into_iter().map(|arg| arg.into()));
        self
    }

    pub fn scp_path(mut self, p: Box<Path>) -> Self {
        self.scp_path = Some(p);
        self
    }

    pub fn check_features(self) -> Result<u64, SyzError> {
        let builder = self.extra_arg("check");
        let mut syz = builder.syz_cmd()?;
        let output = syz.output()?;
        if output.status.success() {
            let out = output.stdout;
            assert_eq!(out.len(), 8);
            let mut val = [0; 8];
            val.copy_from_slice(&out[0..]);
            let ret = u64::from_le_bytes(val);
            Ok(ret)
        } else {
            let err = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(SyzError::CheckFeatures(err))
        }
    }

    pub fn do_setup(self, features: u64) -> Result<(), SyzError> {
        let mut builder = self.extra_arg("setup");
        if features & features::FEATURE_LEAK != 0 {
            builder = builder.extra_arg("leak");
        }
        if features & features::FEATURE_FAULT != 0 {
            builder = builder.extra_arg("fault");
        }
        if features & features::FEATURE_KCSAN != 0 {
            builder = builder.extra_arg("kcsan");
        }
        if features & features::FEATURE_USB_EMULATION != 0 {
            builder = builder.extra_arg("usb");
        }

        let mut syz = builder.syz_cmd()?;
        let output = syz.output()?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(SyzError::Setup(err))
        } else {
            Ok(())
        }
    }

    pub fn spawn(self) -> Result<SyzHandle, SyzError> {
        let pid = self
            .pid
            .ok_or_else(|| SyzError::Config("need pid".to_string()))?;
        let mut syz = self.syz_cmd()?;
        if self.use_shm {
            syz.arg("use-ivshm");
        }

        let mut syz = syz.spawn()?;

        let stdin = syz.stdin.take().unwrap();
        let stdout = syz.stdout.take().unwrap();
        let stderr = LogReader::new(into_async_file(syz.stderr.take().unwrap()));
        let mut syz_handle = SyzHandle {
            syz,
            stdin,
            stdout,
            pid,
            stderr,
            use_shm: self.use_shm,
            env_flags: self.env_flags,
        };

        if self.use_forksrv {
            if let Err(e) = syz_handle.handshake() {
                let stderr = String::from_utf8(syz_handle.output()).unwrap_or_default();
                return Err(SyzError::HandShake(format!("{}\nSTDERR:\n{}", e, stderr)));
            }
        } else {
            // use fork server by default for now.
            todo!()
        }
        Ok(syz_handle)
    }

    fn syz_cmd(&self) -> Result<Command, SyzError> {
        let from = self
            .executor
            .as_deref()
            .ok_or_else(|| SyzError::Config("need executable file".to_string()))?;
        let bin = from.file_name().ok_or_else(|| {
            SyzError::Config(format!("bad executable file path: {}", from.display()))
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
            .ok_or_else(|| SyzError::Config("need ssh ip".to_string()))?;
        let ssh_port = self
            .ssh_port
            .ok_or_else(|| SyzError::Config("need ssh port".to_string()))?;
        let ssh_key = self
            .ssh_key
            .as_ref()
            .ok_or_else(|| SyzError::Config("need key".to_string()))?;
        let ssh_user = self.ssh_user.as_deref().unwrap_or("root");

        if self.copy_bin {
            scp(ssh_ip, ssh_port, ssh_key, ssh_user, from, &to)?;
        }

        let mut cmd = ssh_basic_cmd(ssh_ip, ssh_port, ssh_key, ssh_user);
        cmd.arg(to)
            .args(&self.extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(cmd)
    }
}

pub struct SyzHandle {
    pub(crate) syz: Child,
    pub(crate) stdin: ChildStdin,
    pub(crate) stdout: ChildStdout,
    pub(crate) stderr: LogReader,
    pub(crate) pid: u64,
    pub(crate) use_shm: bool,
    pub(crate) env_flags: EnvFlags,
}

pub enum SyzExecResult {
    Ok(Vec<CallExecInfo>),
    Failed {
        info: Vec<CallExecInfo>,
        err: Box<dyn Error + 'static>,
    },
    Internal(Box<dyn Error + 'static>),
}

impl SyzHandle {
    pub fn exec(
        &mut self,
        t: &Target,
        p: &Prog,
        opt: &ExecOpt,
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
            let (stderr, _) = self.stderr.read_to_string();
            if exit_status == SYZ_STATUS_INTERNAL_ERROR {
                return SyzExecResult::Internal(stderr.into());
            } else {
                err = format!("exec error: {}\nsyz stderr: {}", e, stderr);
            }
            failed = true;
        }
        if !failed {
            self.stderr.clear();
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
        let (out, _) = self.stderr.read_all();
        out
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
