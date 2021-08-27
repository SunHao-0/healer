use crate::{
    crash::Crash,
    feedback::Feedback,
    fuzzer::{Fuzzer, SharedState, HISTORY_CAPACITY},
    stats::Stats,
    util::stop_req,
};
use anyhow::Context;
use healer_core::{
    corpus::CorpusWrapper,
    prog::Prog,
    relation::{Relation, RelationWrapper},
    target::Target,
    HashSet,
};
use healer_vm::{
    qemu::{QemuConfig, QemuHandle},
    ssh::ssh_basic_cmd,
};
use rand::{prelude::SmallRng, SeedableRng};
use shared_memory::{Shmem, ShmemConf, ShmemError};
use std::{
    collections::VecDeque,
    os::raw::c_int,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    thread,
    time::Duration,
};
use syz_wrapper::{
    exec::{
        default_env_flags,
        features::{detect_features, features_to_env_flags, FEATURES_NAME},
        ExecConfig, ExecutorHandle, IN_SHM_SZ, OUT_SHM_SZ,
    },
    sys::{load_target, target_exec_use_forksrv, target_exec_use_shm, SysTarget},
};

pub mod arch;
pub mod crash;
pub mod feedback;
pub mod fuzzer;
pub mod stats;
pub mod util;

pub struct Config {
    pub os: String,
    pub relations: Option<PathBuf>,
    pub input_prog: Option<PathBuf>,
    pub crash_whitelist: Option<PathBuf>,
    pub job: usize,
    pub syz_dir: PathBuf,

    pub qemu_config: QemuConfig,
    pub exec_config: Option<ExecConfig>,
}

unsafe impl Send for Config {} // do not send config while holding shm.
unsafe impl Sync for Config {}

impl Clone for Config {
    fn clone(&self) -> Self {
        let exec_config = if let Some(old) = self.exec_config.as_ref() {
            let config = ExecConfig {
                pid: u64::MAX,
                env: old.env,
                shms: None,
                use_forksrv: old.use_forksrv,
            };
            Some(config)
        } else {
            None
        };
        Self {
            os: self.os.clone(),
            relations: self.relations.clone(),
            input_prog: self.input_prog.clone(),
            crash_whitelist: self.crash_whitelist.clone(),
            job: self.job,
            syz_dir: self.syz_dir.clone(),
            qemu_config: self.qemu_config.clone(),
            exec_config,
        }
    }
}

impl Config {
    pub fn check(&self) -> anyhow::Result<()> {
        todo!()
    }

    pub fn syz_executor(&self) -> PathBuf {
        self.syz_dir.join("bin").join("syz-executor")
    }
}

pub fn boot(mut config: Config) -> anyhow::Result<()> {
    config.check().context("config error")?;
    let target_name = format!("{}/{}", config.os, arch::TARGET_ARCH);
    log::info!("loading target {}...", target_name);
    let target = load_target(&target_name).context("failed to load target")?;
    let sys_target = SysTarget::from_str(&target_name).unwrap();
    config.exec_config = Some(ExecConfig {
        pid: 0,
        env: default_env_flags(false, "none"),
        shms: None,
        use_forksrv: target_exec_use_forksrv(sys_target),
    });

    let mut relation = Relation::new(&target);
    if let Some(r) = config.relations.as_ref() {
        log::info!("loading extra relations...");
        load_extra_relations(r, &mut relation, &target)
            .context("failed to load extra reltaions")?;
    }

    let mut input_progs = Vec::new();
    if let Some(p) = config.input_prog.as_ref() {
        log::info!("loading input progs");
        let progs = load_progs(p, &target).context("failed to load input progs")?;
        input_progs = split_input_progs(progs, config.job);
    }

    let crash = if let Some(l) = config.crash_whitelist.as_ref() {
        log::info!("loading crash whitelist");
        let l = load_crash_whitelist(l).context("failed to load crash whitelist")?;
        Crash::with_whitelist(l)
    } else {
        Crash::new()
    };

    log::info!("pre booting one vm...");
    if target_exec_use_shm(sys_target) {
        setup_fuzzer_shm(0, &mut config).context("failed to setup shm")?;
    }
    let mut qemu = QemuHandle::with_config(config.qemu_config.clone());
    let boot_duration = qemu.boot().context("failed to boot qemu")?;
    log::info!("boot cost around {}s", boot_duration.as_secs());
    let (vm_ip, vm_port) = qemu.addr().unwrap();
    let (ssh_key, ssh_user) = qemu.ssh().unwrap();
    let mut ssh = ssh_basic_cmd(vm_ip, vm_port, ssh_key, ssh_user);
    ssh.arg(&config.syz_executor());
    log::info!("detecting features");
    let features = detect_features(&mut ssh).context("failed to detect features")?;
    for (i, feature) in FEATURES_NAME.iter().enumerate() {
        if features & (1 << i) != 0 {
            log::info!("{:<28}: enabled", feature);
        }
    }
    features_to_env_flags(features, &mut config.exec_config.as_mut().unwrap().env);

    let stats = Arc::new(Stats::new());
    let shared_state = SharedState {
        target: Arc::new(target),
        relation: Arc::new(RelationWrapper::new(relation)),
        corpus: Arc::new(CorpusWrapper::new()),
        stats: Arc::clone(&stats),
        feedback: Arc::new(Feedback::new()),
        crash: Arc::new(crash),
    };

    let mut fuzzers = Vec::new();
    for id in 1..config.job {
        let progs = input_progs.pop().unwrap();
        let shared_state = SharedState::clone(&shared_state);
        let mut fuzzer_config = config.clone();
        let handle = thread::spawn(move || {
            fuzzer_config.exec_config.as_mut().unwrap().pid = id as u64;
            setup_fuzzer_shm(id, &mut fuzzer_config)
                .with_context(|| format!("failed to setup shm for fuzzer-{}", id))?;
            let mut qemu = QemuHandle::with_config(fuzzer_config.qemu_config.clone());
            qemu.boot()
                .with_context(|| format!("failed boot qemu for fuzzer-{}", id))?;
            let mut executor =
                ExecutorHandle::with_config(fuzzer_config.exec_config.take().unwrap());
            spawn_syz(&fuzzer_config, &qemu, &mut executor)
                .with_context(|| format!("failed to spawn executor for fuzzer-{}", id))?;
            let mut fuzzer = Fuzzer {
                shared_state,
                id,
                rng: SmallRng::from_entropy(),
                executor,
                qemu,
                run_history: VecDeque::with_capacity(HISTORY_CAPACITY),
                config: fuzzer_config,
            };

            fuzzer.fuzz_loop(progs)
        });
        fuzzers.push(handle);
    }

    // run fuzzer-0
    let progs = input_progs.pop().unwrap();
    debug_assert!(input_progs.is_empty());
    let handle = thread::spawn(move || {
        let mut executor = ExecutorHandle::with_config(config.exec_config.take().unwrap());
        spawn_syz(&config, &qemu, &mut executor)
            .with_context(|| format!("failed to spawn executor for fuzzer-{}", 0))?;
        let mut fuzzer = Fuzzer {
            shared_state,
            id: 0,
            rng: SmallRng::from_entropy(),
            executor,
            qemu,
            run_history: VecDeque::with_capacity(HISTORY_CAPACITY),
            config,
        };
        fuzzer.fuzz_loop(progs)
    });
    fuzzers.push(handle);

    thread::spawn(move || {
        stats.report(Duration::from_secs(10));
    });

    setup_signal_handler();
    todo!()
}

fn setup_signal_handler() {
    use signal_hook::consts::*;
    use signal_hook::iterator::exfiltrator::WithOrigin;
    use signal_hook::iterator::SignalsInfo;

    fn named_signal(sig: c_int) -> String {
        signal_hook::low_level::signal_name(sig)
            .map(|n| format!("{}({})", n, sig))
            .unwrap_or_else(|| sig.to_string())
    }

    std::thread::spawn(move || {
        let mut signals = SignalsInfo::<WithOrigin>::new(TERM_SIGNALS).unwrap();

        let info = signals.into_iter().next().unwrap();
        let from = if let Some(p) = info.process {
            format!("(pid: {}, uid: {})", p.pid, p.uid)
        } else {
            "unknown".to_string()
        };
        log::info!(
            "{} recved, from: {}, cause: {:?}",
            named_signal(info.signal),
            from,
            info.cause
        );
        println!("Waiting fuzzers to exit...");
        stop_req();
    });
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("format: {0}")]
    Format(String),
}

fn load_extra_relations(
    _path: &Path,
    _relation: &mut Relation,
    _target: &Target,
) -> Result<(), LoadError> {
    todo!()
}

fn load_progs(_path: &Path, _target: &Target) -> Result<Vec<Prog>, LoadError> {
    todo!()
}

fn load_crash_whitelist(_path: &Path) -> Result<HashSet<String>, LoadError> {
    todo!()
}

fn setup_fuzzer_shm(fuzzer_id: usize, config: &mut Config) -> anyhow::Result<()> {
    let in_shm_id = format!("healer-in_shm-{}-{}", fuzzer_id, std::process::id());
    let out_shm_id = format!("healer-out_shm_{}-{}", fuzzer_id, std::process::id());
    let in_shm = create_shm(&in_shm_id, IN_SHM_SZ).context("failed to create input shm")?;
    let out_shm = create_shm(&out_shm_id, OUT_SHM_SZ).context("failed to create outpout shm")?;
    config
        .qemu_config
        .add_shm(&in_shm_id, IN_SHM_SZ)
        .add_shm(&out_shm_id, OUT_SHM_SZ);
    config.exec_config.as_mut().unwrap().shms = Some((in_shm, out_shm));
    Ok(())
}

fn create_shm(id: &str, sz: usize) -> anyhow::Result<Shmem> {
    match ShmemConf::new().os_id(id).size(sz).create() {
        Ok(mut shm) => {
            shm.set_owner(true);
            Ok(shm)
        }
        Err(ShmemError::MappingIdExists) => {
            let mut shm = ShmemConf::new().os_id(id).size(sz).open()?;
            shm.set_owner(true);
            Ok(shm)
        }
        Err(e) => Err(e.into()),
    }
}

fn spawn_syz(config: &Config, qemu: &QemuHandle, exec: &mut ExecutorHandle) -> anyhow::Result<()> {
    let (vm_ip, vm_port) = qemu.addr().unwrap();
    let (ssh_key, ssh_user) = qemu.ssh().unwrap();
    let mut ssh = ssh_basic_cmd(vm_ip, vm_port, ssh_key, ssh_user);
    ssh.arg(config.syz_executor()).arg("use-ivshm"); // use ivshm mode.
    exec.spawn(ssh).map_err(|e| e.into())
}

fn split_input_progs(_progs: Vec<Prog>, _job: usize) -> Vec<Vec<Prog>> {
    todo!()
}
