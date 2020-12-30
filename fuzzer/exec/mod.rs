use std::path::{Path, PathBuf};

use hlang::ast::Prog;
use iota::iota;
use qemu::{QemuConf, QemuHandle};
use shared_memory::{Shmem, ShmemConf, ShmemError};
use ssh::SshConf;
use syz::{EnvFlags, SyzHandle, SyzHandleBuilder};
use thiserror::Error;

use crate::target::Target;

use self::syz::ExecOpt;

/// Communication with syz-executor.
pub mod comm;
/// Spawning qemu.
pub mod qemu;
/// Prog Serialization.
pub mod serialize;
/// Invoking ssh.
pub mod ssh;
/// Syz-executor handling.
pub mod syz;

pub enum ExecResult {
    Normal(Vec<CallExecInfo>),
    Failed {
        info: Vec<CallExecInfo>,
        err: Box<dyn std::error::Error + 'static>,
    },
    Crash(CrashInfo), // TODO use structural crash information.
}

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("syz-executor: {0}")]
    SyzInternal(Box<dyn std::error::Error + 'static>),
    #[error("spawn: {0}")]
    Spawn(#[from] SpawnError),
}

pub struct CrashInfo {
    qemu_stdout: Vec<u8>,
    qemu_stderr: Vec<u8>,
    syz_out: String,
}

pub type CallFlags = u32;

iota! {
    const CALL_EXECUTED : CallFlags = 1 << (iota); // was started at all
    , CALL_FINISHED                                // finished executing (rather than blocked forever)
    , CALL_BLOCKED                                 // finished but blocked during execution
    , CALL_FAULT_INJECTED                          // fault was injected into this call
}

#[derive(Debug, Default, Clone)]
pub struct CallExecInfo {
    pub(crate) flags: CallFlags,
    pub(crate) branches: Vec<u32>,
    pub(crate) blocks: Vec<u32>,
    pub(crate) errno: i32,
}

pub struct ExecConf {
    pub executor: Box<Path>,
    pub use_shm: bool,
    pub use_forksrv: bool,
}

pub struct ExecHandle {
    qemu: Option<QemuHandle>,
    syz: Option<SyzHandle>,

    in_shm: Option<Shmem>,
    out_shm: Option<Shmem>,
    in_mem: Option<Box<[u8]>>,
    out_mem: Option<Box<[u8]>>,

    qemu_conf: QemuConf,
    ssh_conf: SshConf,
    exec_conf: ExecConf,
    env: EnvFlags,
    pid: u64,
}

impl ExecHandle {
    pub fn exec(&mut self, t: &Target, p: &Prog, opt: ExecOpt) -> Result<ExecResult, ExecError> {
        if self.syz.is_none() {
            self.spawn_syz()?;
        }

        let syz = self.syz.as_mut().unwrap();
        let exec_result = if self.exec_conf.use_shm {
            let in_shm = unsafe { self.in_shm.as_mut().unwrap().as_slice_mut() };
            let out_shm = unsafe { self.out_shm.as_mut().unwrap().as_slice_mut() };
            syz.exec(t, p, opt, in_shm, out_shm)
        } else {
            let in_mem = self.in_mem.as_deref_mut().unwrap();
            let out_mem = self.out_mem.as_deref_mut().unwrap();
            syz.exec(t, p, opt, in_mem, out_mem)
        };

        match exec_result {
            syz::SyzExecResult::Ok(info) => Ok(ExecResult::Normal(info)),
            syz::SyzExecResult::Failed { info, err } => {
                let syz = self.syz.take().unwrap();
                drop(syz);
                if !self
                    .qemu
                    .as_mut()
                    .unwrap()
                    .is_alive()
                    .map_err(SpawnError::IO)?
                {
                    let qemu = self.qemu.take().unwrap();
                    let (stdout, stderr) = qemu.output();
                    let crash_info = CrashInfo {
                        qemu_stderr: stderr,
                        qemu_stdout: stdout,
                        syz_out: err.to_string(),
                    };
                    Ok(ExecResult::Crash(crash_info))
                } else {
                    Ok(ExecResult::Failed { info, err })
                }
            }
            syz::SyzExecResult::Internal(e) => Err(ExecError::SyzInternal(e)),
        }
    }

    fn spawn_syz(&mut self) -> Result<(), SpawnError> {
        if self.qemu.is_none() {
            self.qemu = Some(qemu::boot(&self.qemu_conf, &self.ssh_conf)?);
        }
        let ssh_conf = &self.ssh_conf;
        let conf = &self.exec_conf;
        let qemu = self.qemu.as_ref().unwrap();
        let syz = SyzHandleBuilder::new()
            .ssh_addr(qemu.ssh_ip(), qemu.ssh_port())
            .ssh_identity(
                ssh_conf.ssh_key.display().to_string(),
                ssh_conf.ssh_user.clone().unwrap(),
            )
            .use_forksrv(conf.use_forksrv)
            .user_shm(conf.use_shm)
            .executor(conf.executor.clone())
            .copy_bin(true)
            .pid(self.pid)
            .env_flags(self.env)
            .spawn()?;
        self.syz = Some(syz);
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to boot qemu: {0}")]
    Qemu(#[from] qemu::BootError),
    #[error("failed to spawn syz-executor: {0}")]
    Syz(#[from] syz::SyzSpawnError),
    #[error("failed to use shm: {0}")]
    Shm(#[from] ShmemError),
    #[error("io: {0}")]
    IO(#[from] std::io::Error),
}

pub fn spawn_in_qemu(
    conf: ExecConf,
    mut qemu_conf: QemuConf,
    mut ssh_conf: SshConf,
    pid: u64,
) -> Result<ExecHandle, SpawnError> {
    use syz::*;
    const ENV: u64 = FLAG_SIGNAL | FLAG_ENABLE_TUN | FLAG_ENABLE_NETDEV | FLAG_ENABLE_CGROUPS;
    const IN_MEM_SIZE: usize = 4 << 20;
    const OUT_MEM_SIZE: usize = 16 << 20;

    let (mut in_shm, mut out_shm) = (None, None);
    let (mut in_mem, mut out_mem) = (None, None);
    if conf.use_shm {
        let in_shm_id = format!("healer-in_shm-{}", pid);
        let out_shm_id = format!("healer-out_shm_{}", pid);
        let shm_dev = PathBuf::from("/dev/shm");
        in_shm = Some(shm(&in_shm_id, IN_MEM_SIZE)?);
        out_shm = Some(shm(&out_shm_id, OUT_MEM_SIZE)?);
        qemu_conf.mem_backend_files.push((
            shm_dev.join(&in_shm_id).into_boxed_path(),
            IN_MEM_SIZE >> 20,
        ));
        qemu_conf.mem_backend_files.push((
            shm_dev.join(&out_shm_id).into_boxed_path(),
            OUT_MEM_SIZE >> 20,
        ));
    } else {
        in_mem = Some(boxed_buf(IN_MEM_SIZE));
        out_mem = Some(boxed_buf(OUT_MEM_SIZE));
    }
    let qemu = qemu::boot(&qemu_conf, &ssh_conf)?;
    if ssh_conf.ssh_user.is_none() {
        let u = "root".to_string();
        ssh_conf.ssh_user = Some(u);
    };

    let mut handle = ExecHandle {
        qemu: Some(qemu),
        syz: None,
        exec_conf: conf,
        qemu_conf,
        ssh_conf,
        env: ENV,
        in_mem,
        out_mem,
        in_shm,
        out_shm,
        pid,
    };
    handle.spawn_syz()?;

    Ok(handle)
}

fn shm<T: AsRef<str>>(id: T, sz: usize) -> Result<Shmem, ShmemError> {
    let id = id.as_ref();
    match ShmemConf::new().os_id(id).size(sz).create() {
        Ok(shm) => Ok(shm),
        Err(ShmemError::MappingIdExists) => ShmemConf::new().os_id(id).size(sz).open(),
        Err(e) => Err(e),
    }
}

fn boxed_buf(sz: usize) -> Box<[u8]> {
    let mut buf: Vec<u8> = Vec::with_capacity(sz);
    unsafe {
        buf.set_len(sz);
    }
    for i in &mut buf {
        *i = 0;
    } // same as memset
    buf.into_boxed_slice()
}
