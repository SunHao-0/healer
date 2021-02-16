use crate::exec::syz::{SyzHandle, SyzHandleBuilder};
use crate::model::Prog;
use crate::targets::Target;
use crate::{exec::qemu::QemuHandle, fuzz::features};

use std::path::{Path, PathBuf};

use iota::iota;
use shared_memory::{Shmem, ShmemConf, ShmemError};
use thiserror::Error;

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
    Crash(CrashInfo), // TODO use structural crash information.
}

/// Internal error of one execution.
#[derive(Debug, Error)]
pub enum ExecError {
    /// Internal error of executor implementation.
    #[error("syz-executor: {0}")]
    SyzInternal(Box<dyn std::error::Error + 'static>),
    /// Spawning error due to system error.
    #[error("spawn: {0}")]
    Spawn(#[from] SpawnError),
}

/// Raw crash information.
pub struct CrashInfo {
    /// Stdout of qemu.
    pub qemu_stdout: Vec<u8>,
    /// Stdin of qemu.
    pub qemu_stderr: Vec<u8>,
    /// stderr of inner executor.
    pub syz_out: String,
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
#[derive(Debug, Clone)]
pub struct ExecOpt {
    pub flags: ExecFlags,
    /// Inject fault for 'fault_call'.
    pub fault_call: i32,
    /// Inject fault 'nth' for 'fault_call'
    pub fault_nth: i32,
}

impl Default for ExecOpt {
    fn default() -> Self {
        Self::new()
    }
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

/// Hign-level controller of inner executor and qemu.
pub struct ExecHandle {
    target: Target,
    /// Inner qemu driver, handling qemu boot, interactive stuff.
    qemu: Option<QemuHandle>,
    /// Inner executor driver, handling spawn, communication stuff.
    syz: Option<SyzHandle>,
    /// Input shared memory for inner executor. Value is None if use_shm is false.
    in_shm: Option<Shmem>,
    /// Output shared memory for inner executor. Value is None if use_shm is false.
    out_shm: Option<Shmem>,
    /// Input buffer for inner executor. Value is None if use shm.
    in_mem: Option<Box<[u8]>>,
    /// Input shared memory for inner executor. Value is None if use shm.
    out_mem: Option<Box<[u8]>>,
    /// Configuration of qemu, such as image path, boot target.
    qemu_conf: QemuConf,
    /// Configuration of ssh, such as addr, identity.
    ssh_conf: SshConf,
    /// Configuration of executor, such as executor path.
    exec_conf: ExecConf,
    /// Env configuration of inner executor.
    env: Option<EnvFlags>,
    features: Option<u64>,
    /// Unique id for inner executor, not process id(linux pid).
    pid: u64,
    /// Copy inner executor executable file or not.
    copy_bin: bool,
}

impl ExecHandle {
    /// Execute one prog with specific option.
    pub fn exec(&mut self, opt: &ExecOpt, p: &Prog) -> Result<ExecResult, ExecError> {
        if self.syz.is_none() {
            self.spawn_syz()?;
        }

        let syz = self.syz.as_mut().unwrap();
        let exec_result = if self.target.syz_exec_use_shm {
            let in_shm = unsafe { self.in_shm.as_mut().unwrap().as_slice_mut() };
            let out_shm = unsafe { self.out_shm.as_mut().unwrap().as_slice_mut() };
            syz.exec(&self.target, p, opt, in_shm, out_shm)
        } else {
            let in_mem = self.in_mem.as_deref_mut().unwrap();
            let out_mem = self.out_mem.as_deref_mut().unwrap();
            syz.exec(&self.target, p, opt, in_mem, out_mem)
        };

        let ret = match exec_result {
            syz::SyzExecResult::Ok(info) => Ok(ExecResult::Normal(info)),
            syz::SyzExecResult::Failed { info, err } => {
                self.syz.take().unwrap();
                if !self
                    .qemu
                    .as_mut()
                    .unwrap()
                    .is_alive()
                    .map_err(SpawnError::IO)?
                {
                    let qemu = self.qemu.take().unwrap();
                    self.copy_bin = true; // -snapshot is enabled.
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
            syz::SyzExecResult::Internal(e) => {
                if e.downcast_ref::<serialize::SerializeError>().is_none() {
                    self.syz.take().unwrap();
                }
                Err(ExecError::SyzInternal(e))
            }
        };

        if let Some(ref mut qemu) = self.qemu {
            qemu.clear();
        }
        ret
    }

    pub fn check_features(&mut self) -> Result<u64, SpawnError> {
        if let Some(features) = self.features {
            return Ok(features);
        }

        let features = self.syz_builder().check_features()?;
        self.features = Some(features);
        self.env = Some(new_env_flags(features));
        if let Some(syz) = self.syz.as_mut() {
            syz.env_flags = self.env.unwrap();
        }
        Ok(features)
    }

    pub fn setup_env(&mut self) -> Result<(), SpawnError> {
        if self.qemu.is_none() {
            self.qemu = Some(qemu::boot(&self.qemu_conf, &self.ssh_conf)?);
        }

        if self.features.is_none() {
            self.check_features()?;
        }

        self.syz_builder()
            .do_setup(self.features.unwrap())
            .map_err(From::from)
    }

    pub fn restart(&mut self) -> Result<(), SpawnError> {
        self.syz = None;
        self.qemu = None;
        self.copy_bin = true;
        self.spawn_syz()
    }

    pub fn spawn_syz(&mut self) -> Result<(), SpawnError> {
        if self.qemu.is_none() {
            self.qemu = Some(qemu::boot(&self.qemu_conf, &self.ssh_conf)?);
            self.setup_env()?;
        }
        self.syz = Some(self.syz_builder().spawn()?);
        self.copy_bin = false;
        Ok(())
    }

    fn syz_builder(&self) -> SyzHandleBuilder {
        let ssh_conf = &self.ssh_conf;
        let conf = &self.exec_conf;
        let qemu = self.qemu.as_ref().unwrap();
        let mut builder = SyzHandleBuilder::new()
            .ssh_addr(qemu.ssh_ip(), qemu.ssh_port())
            .ssh_identity(
                ssh_conf.ssh_key.display().to_string(),
                ssh_conf.ssh_user.clone().unwrap(),
            )
            .use_forksrv(self.target.syz_exec_use_forksrv)
            .use_shm(self.target.syz_exec_use_shm)
            .executor(conf.executor.clone())
            .copy_bin(self.copy_bin)
            .pid(self.pid);
        if let Some(env) = self.env {
            builder = builder.env_flags(env);
        }
        builder
    }
}

#[derive(Debug, Error)]
pub enum SpawnError {
    #[error("failed to boot qemu: {0}")]
    Qemu(#[from] qemu::BootError),
    #[error("failed to spawn syz-executor: {0}")]
    Syz(#[from] syz::SyzError),
    #[error("failed to use shm: {0}")]
    Shm(#[from] ShmemError),
    #[error("io: {0}")]
    IO(#[from] std::io::Error),
}

/// Env flags to executor.
type EnvFlags = u64; // TODO this should only be public to super module, but iota crate doesn't support pub(crate) token.

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
    , FLAG_ENABLE_CLOSEFDS                          // close fds after each program
    , FLAG_ENABLE_DEVLINKPCI                         // setup devlink PCI device
    , FLAG_ENABLE_VHCI_INJECTION                     // setup and use /dev/vhci for hci packet injection
    , FLAG_ENABLE_WIFI                               // setup and use mac80211_hwsim for wifi emulation
}

fn new_env_flags(features: u64) -> EnvFlags {
    let mut env = FLAG_SIGNAL;

    if features & features::FEATURE_EXTRA_COVERAGE != 0 {
        env |= FLAG_EXTRA_COVER;
    }
    if features & features::FEATURE_NET_INJECTION != 0 {
        env |= FLAG_ENABLE_TUN;
    }
    if features & features::FEATURE_NET_DEVICES != 0 {
        env |= FLAG_ENABLE_NETDEV;
    }

    env |= FLAG_ENABLE_NETRESET;
    env |= FLAG_ENABLE_CGROUPS;
    env |= FLAG_ENABLE_CLOSEFDS;

    if features & features::FEATURE_DEVLINK_PCI != 0 {
        env |= FLAG_ENABLE_DEVLINKPCI;
    }
    if features & features::FEATURE_VHCI_INJECTION != 0 {
        env |= FLAG_ENABLE_VHCI_INJECTION;
    }
    if features & features::FEATURE_WIFI_EMULATION != 0 {
        env |= FLAG_ENABLE_WIFI;
    }

    env
}

/// Configuration of executor.
#[derive(Debug, Clone)]
pub struct ExecConf {
    /// Path to inner executor executable file.
    pub executor: Box<Path>,
}

impl Default for ExecConf {
    fn default() -> Self {
        Self {
            executor: PathBuf::from("./syz-executor").into_boxed_path(),
        }
    }
}

/// Configuration of booting qemu.
#[derive(Debug, Clone)]
pub struct QemuConf {
    /// Booting target, such as linux/amd64, see qemu.rs for all supported target.
    pub target: String,
    /// Path tp disk image to boot, default is "stretch.img".
    pub img_path: Box<Path>,
    /// Optional Path to kernel bzImage.
    pub kernel_path: Option<Box<Path>>,
    /// Smp, default is 2.
    pub smp: u8,
    /// Mem size in megabyte.
    pub mem: u32,
    /// Shared memory device file path, creadted automatically if use qemu ivshm.
    pub mem_backend_files: Vec<(Box<Path>, usize)>,
}

impl Default for QemuConf {
    fn default() -> Self {
        Self {
            target: "linux/amd64".to_string(),
            kernel_path: None,
            img_path: PathBuf::from("./stretch.img").into_boxed_path(),
            smp: 2,
            mem: 2048,
            mem_backend_files: Vec::new(),
        }
    }
}

/// Configuration of ssh.
#[derive(Debug, Clone)]
pub struct SshConf {
    /// Path to temporary secret key, for ssh -i option.
    pub ssh_key: Box<Path>,
    /// Ssh user, default is root.
    pub ssh_user: Option<String>,
}

impl Default for SshConf {
    fn default() -> Self {
        Self {
            ssh_key: PathBuf::from("./stretch.id_rsa").into_boxed_path(),
            ssh_user: Some("root".to_string()),
        }
    }
}

/// Boot qemu with 'qemu_conf' and 'ssh_conf', then spawn inner executor in it.
pub fn spawn_in_qemu(
    conf: ExecConf,
    mut qemu_conf: QemuConf,
    mut ssh_conf: SshConf,
    pid: u64,
) -> Result<ExecHandle, SpawnError> {
    // TODO use env detection to decide this.
    const IN_MEM_SIZE: usize = 4 << 20;
    const OUT_MEM_SIZE: usize = 16 << 20;

    let target = Target::new(&qemu_conf.target).unwrap();
    let (mut in_shm, mut out_shm) = (None, None);
    let (mut in_mem, mut out_mem) = (None, None);
    if target.syz_exec_use_shm {
        let in_shm_id = format!("healer-in_shm-{}-{}", pid, std::process::id());
        let out_shm_id = format!("healer-out_shm_{}-{}", pid, std::process::id());
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
    if ssh_conf.ssh_user.is_none() {
        let u = "root".to_string();
        ssh_conf.ssh_user = Some(u);
    };

    let mut handle = ExecHandle {
        target,
        qemu: None,
        syz: None,
        exec_conf: conf,
        qemu_conf,
        ssh_conf,
        env: None,
        in_mem,
        out_mem,
        in_shm,
        out_shm,
        pid,
        copy_bin: true,
        features: None,
    };
    handle.spawn_syz()?;

    Ok(handle)
}

fn shm<T: AsRef<str>>(id: T, sz: usize) -> Result<Shmem, ShmemError> {
    let id = id.as_ref();
    match ShmemConf::new().os_id(id).size(sz).create() {
        Ok(mut shm) => {
            shm.set_owner(true);
            Ok(shm)
        }
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
