//! Start, interactive with syz-executor

use hlang::ast::Prog;
use iota::iota;
use std::{
    fmt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
};

use super::{
    comm::handshake,
    ssh::{scp, ssh_basic_cmd, ScpError},
    ExecResult,
};
use crate::bg_task::Reader;

#[derive(Debug)]
pub enum SyzSpawnError {
    Config(String),
    Spawn(std::io::Error),
    Scp(ScpError),
    HandShake(String),
}

impl fmt::Display for SyzSpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyzSpawnError::Config(ref err) => write!(f, "config: {}", err),
            SyzSpawnError::Spawn(ref err) => write!(f, "spawn: {}", err),
            SyzSpawnError::Scp(ref err) => write!(f, "copy syz-executor: {}", err),
            SyzSpawnError::HandShake(ref err) => write!(f, "waiting handshake: {}", err),
        }
    }
}

impl From<std::io::Error> for SyzSpawnError {
    fn from(err: std::io::Error) -> Self {
        SyzSpawnError::Spawn(err)
    }
}

impl From<ScpError> for SyzSpawnError {
    fn from(err: ScpError) -> Self {
        SyzSpawnError::Scp(err)
    }
}

/// Env flags to executor.
pub type EnvFlags = u64;

iota! {
    pub const FLAG_DEBUG: EnvFlags = 1 << (iota);             // debug output from executor
    , FLAG_SIGNAL                                    // collect feedback signals (coverage)
    , FLAG_SANDBOX_SETUID                            // impersonate nobody user
    , FLAG_SANDBOX_NAMESPACE                         // use namespaces for sandboxing
    , FLAG_SANDBOX_ANDROID                           // use Android sandboxing for the untrusted_app domain
    , FLAG_EXTRA_COVER                               // collect extra coverage
    , FLAG_ENABLE_TUN                                // setup and use /dev/tun for packet injection
    , FLAG_ENABLE_NETDEV                             // setup more network devices for testing
    , FLAG_ENABLE_NETRESET                           // reset network namespace between programs
    , FLAG_ENABLE_CGROUPS                            // setup cgroups for testing
    , FLAG_ENABLE_CLOSEFDS                          // close fds after each program
    , FLAG_ENABLE_DEVLINKPCI                         // setup devlink PCI device
    , FLAG_ENABLE_VHCI_INJECTION                     // setup and use /dev/vhci for hci packet injection
    , FLAG_ENABLE_WIFI                               // setup and use mac80211_hwsim for wifi emulation
}

pub struct SyzHandleBuilder {
    ssh_key: Option<String>,
    ssh_user: Option<String>,
    ssh_ip: Option<String>,
    ssh_port: Option<u16>,
    executor: Option<Box<Path>>,
    env_flags: EnvFlags,
    use_forksrv: bool,
    copy_bin: bool,
    extra_args: Vec<String>,
    scp_path: Option<Box<Path>>,
    pid: Option<u64>,
}

impl SyzHandleBuilder {
    pub fn new() -> Self {
        Self {
            ssh_key: None,
            ssh_user: None,
            ssh_ip: None,
            ssh_port: None,
            executor: None,
            env_flags: FLAG_SIGNAL,
            use_forksrv: true,
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

    pub fn executor(mut self, p: Box<Path>) -> Self {
        self.executor = Some(p);
        self
    }

    pub fn use_forksrv(mut self, u: bool) -> Self {
        self.use_forksrv = u;
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

    pub fn spawn(self) -> Result<SyzHandle, SyzSpawnError> {
        let pid = self.pid.ok_or(SyzSpawnError::Config(format!("need pid")))?;
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
        };

        if self.use_forksrv {
            if let Err(e) = handshake(
                &mut syz_handle.stdin,
                &mut syz_handle.stdout,
                self.env_flags,
                pid,
            ) {
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
            .ok_or(SyzSpawnError::Config(format!("need executable file")))?;
        let bin = from.file_name().ok_or(SyzSpawnError::Config(format!(
            "bad executable file path: {}",
            from.display()
        )))?;
        let mut to = if let Some(p) = self.scp_path.as_ref() {
            p.to_path_buf()
        } else {
            PathBuf::from("~")
        };
        to.push(&bin);

        let ssh_ip = self
            .ssh_ip
            .as_deref()
            .ok_or(SyzSpawnError::Config(format!("need ssh ip")))?;
        let ssh_port = self
            .ssh_port
            .ok_or(SyzSpawnError::Config(format!("need ssh port")))?;
        let ssh_key = self
            .ssh_key
            .as_ref()
            .ok_or(SyzSpawnError::Config(format!("need key")))?;
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

pub struct ExecOpt {
    pub(crate) flags: ExecFlags,
    pub(crate) use_shm: bool,
    pub(crate) fault_call: i32,
    pub(crate) fault_nth: i32,
}

pub struct SyzHandle {
    syz: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    bg_stderr: Reader,
    pid: u64,
}

impl SyzHandle {
    pub fn exec(&mut self, _: &Prog, flags: ExecFlags) -> Result<ExecResult, ()> {
        todo!()
    }

    pub fn output(mut self) -> Vec<u8> {
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
