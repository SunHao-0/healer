//! Healer fuzz

#[macro_use]
pub mod fuzzer_log;
pub mod arch;
pub mod config;
pub mod crash;
pub mod feedback;
pub mod fuzzer;
pub mod stats;
pub mod util;

use crate::{
    crash::CrashManager,
    feedback::Feedback,
    fuzzer::{Fuzzer, SharedState, HISTORY_CAPACITY},
    stats::Stats,
    util::stop_req,
};
use anyhow::Context;
use config::Config;
use healer_core::{
    corpus::CorpusWrapper,
    parse::parse_prog,
    prog::Prog,
    relation::{Relation, RelationWrapper},
    target::Target,
    HashSet,
};
use healer_vm::{qemu::QemuHandle, ssh::ssh_basic_cmd};
use rand::{
    prelude::{SliceRandom, SmallRng},
    SeedableRng,
};
use shared_memory::{Shmem, ShmemConf, ShmemError};
use std::{
    collections::VecDeque,
    fs::{read_dir, read_to_string},
    os::raw::c_int,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};
use syz_wrapper::{
    exec::{
        default_env_flags,
        features::{detect_features, features_to_env_flags, setup_features, FEATURES_NAME},
        ExecConfig, ExecutorHandle, IN_SHM_SZ, OUT_SHM_SZ,
    },
    sys::{load_target, target_exec_use_forksrv, target_exec_use_shm, SysTarget},
};

pub fn boot(mut config: Config) -> anyhow::Result<()> {
    config.check().context("config error")?;
    println!("{}", HEALER);

    let target_name = format!("{}/{}", config.os, arch::TARGET_ARCH);
    log::info!("loading target {}...", target_name);
    let target = load_target(&target_name).context("failed to load target")?;
    let sys_target = SysTarget::from_str(&target_name).unwrap();
    config.exec_config = Some(ExecConfig {
        pid: 0,
        env: default_env_flags(false, "none"),
        features: 0,
        shms: None,
        use_forksrv: target_exec_use_forksrv(sys_target),
    });
    let stats = Arc::new(Stats::new());

    let mut relation = Relation::new(&target);
    if let Some(r) = config.relations.as_ref() {
        log::info!("loading extra relations...");
        load_extra_relations(r, &mut relation, &target)
            .context("failed to load extra reltaions")?;
        stats.set_re(relation.num() as u64);
    }

    let mut input_progs = Vec::new();
    if let Some(p) = config.input.as_ref() {
        log::info!("loading input progs...");
        // TODO inplace-resume
        let progs = load_progs(p, &target).context("failed to load input progs")?;
        input_progs = split_input_progs(progs, config.job);
    }

    let crash = if let Some(l) = config.crash_whitelist.as_ref() {
        log::info!("loading crash whitelist");
        // TODO inplace-resume
        let l = load_crash_whitelist(l).context("failed to load crash whitelist")?;
        CrashManager::with_whitelist(l, config.output.clone())
    } else {
        CrashManager::new(config.output.clone())
    };

    let shared_state = SharedState {
        target: Arc::new(target),
        relation: Arc::new(RelationWrapper::new(relation)),
        corpus: Arc::new(CorpusWrapper::new()),
        stats: Arc::clone(&stats),
        feedback: Arc::new(Feedback::new()),
        crash: Arc::new(crash),
    };

    log::info!("pre-booting one vm...");
    let use_shm = target_exec_use_shm(sys_target);
    if use_shm {
        setup_fuzzer_shm(0, &mut config).context("failed to setup shm")?;
    }
    config.fixup();
    let mut qemu = QemuHandle::with_config(config.qemu_config.clone());
    let boot_duration = qemu.boot().context("failed to boot qemu")?;
    log::info!("boot cost around {}s", boot_duration.as_secs());

    let remote_exec_path = scp_to_vm(&config.syz_executor(), &qemu)?;
    config.remote_exec = Some(remote_exec_path.clone());
    let ssh_syz = ssh_syz_cmd(&remote_exec_path, &qemu);
    log::info!("detecting features...");
    let features = detect_features(ssh_syz).context("failed to detect features")?;
    config.exec_config.as_mut().unwrap().features = features;
    config.features = Some(features);
    for (i, feature) in FEATURES_NAME.iter().enumerate() {
        if features & (1 << i) != 0 {
            log::info!("{:<28}: enabled", feature);
        }
    }

    log::info!("pre-setup one executor...");
    let ssh_syz = ssh_syz_cmd(&remote_exec_path, &qemu);
    setup_features(ssh_syz, features).context("failed to setup features")?;
    features_to_env_flags(features, &mut config.exec_config.as_mut().unwrap().env);
    let exec_config = config.exec_config.take().unwrap();
    config.exec_config = Some(exec_config.clone()); // clear the shm
    let mut executor = ExecutorHandle::with_config(exec_config);
    spawn_syz(&remote_exec_path, &qemu, &mut executor)
        .context("failed to spawn executor for fuzzer-0")?;
    log::info!("ok, fuzzer-0 should be ready");

    setup_signal_handler();
    thread::spawn(move || {
        stats.report(Duration::from_secs(10));
    });

    let mut fuzzers = Vec::with_capacity(config.job as usize);
    // run fuzzer-0
    let progs = input_progs.pop().unwrap_or_default();
    let config1 = config.clone();
    let shared_state1 = SharedState::clone(&shared_state);
    let handle = thread::spawn(move || {
        let mut fuzzer = Fuzzer {
            shared_state: shared_state1,
            id: 0,
            rng: SmallRng::from_entropy(),
            executor,
            qemu,
            run_history: VecDeque::with_capacity(HISTORY_CAPACITY),
            config,
            last_reboot: Instant::now(),
        };
        fuzzer.fuzz_loop(progs)
    });
    fuzzers.push(handle);
    // skip fuzzer-0
    for id in 1..config1.job {
        let progs = input_progs.pop().unwrap_or_default();
        let shared_state = SharedState::clone(&shared_state);
        let mut fuzzer_config = config1.clone(); // shm removed
        let a = Arc::new(Barrier::new(2));
        let b = Arc::clone(&a);

        let handle = thread::spawn(move || {
            fuzzer_config.exec_config.as_mut().unwrap().pid = id;
            fuzzer_config.fixup();
            if use_shm {
                setup_fuzzer_shm(id, &mut fuzzer_config)
                    .with_context(|| format!("failed to setup shm for fuzzer-{}", id))?;
            }

            let mut qemu = QemuHandle::with_config(fuzzer_config.qemu_config.clone());
            let exec_config = fuzzer_config.exec_config.take().unwrap();
            fuzzer_config.exec_config = Some(exec_config.clone());
            let mut executor = ExecutorHandle::with_config(exec_config);
            prepare_exec_env(&mut fuzzer_config, &mut qemu, &mut executor)?;
            b.wait();
            let mut fuzzer = Fuzzer {
                shared_state,
                id,
                rng: SmallRng::from_entropy(),
                executor,
                qemu,
                run_history: VecDeque::with_capacity(HISTORY_CAPACITY),
                config: fuzzer_config,
                last_reboot: Instant::now(),
            };
            fuzzer.fuzz_loop(progs)
        });

        log::info!("waiting fuzzer-{} online...", id);
        a.wait();
        fuzzers.push(handle);
    }

    let mut err = None;
    for (i, f) in fuzzers.into_iter().enumerate() {
        if let Ok(Err(e)) = f.join() {
            if err.is_none() {
                err = Some("fuzzer exits with errors:".to_string());
            }

            let mut info = format!("\n\tfuzzer-{}: {}", i, e);
            for (i, cause) in e.chain().enumerate() {
                let cause = format!("\n\t\t{}. {}", i, cause);
                info.push_str(&cause);
            }
            err.as_mut().unwrap().push_str(&info);
        }
    }
    if err.is_none() {
        log::info!("All done");
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.unwrap()))
    }
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
        let _ = Command::new("pkill").arg("syz-repro").output(); // stop syz-repro, ignore all errors.
        println!("please waiting fuzzers to exit...");

        stop_req();
    });
}

fn load_extra_relations(
    path: &Path,
    relation: &mut Relation,
    target: &Target,
) -> anyhow::Result<()> {
    let content = read_to_string(path)
        .with_context(|| format!("faield to load extra relations {}", path.display()))?;
    let mut n = 0;
    for l in content.lines().map(|l| l.trim()) {
        if l.is_empty() {
            continue;
        }
        let items = l.split(',').collect::<Vec<_>>();
        if items.len() != 2 {
            continue;
        }
        let a = items[0];
        let b = items[1];
        let sid_a = if let Some(s) = target.syscall_of_name(a) {
            s.id()
        } else {
            continue;
        };
        let sid_b = if let Some(s) = target.syscall_of_name(b) {
            s.id()
        } else {
            continue;
        };
        if relation.insert(sid_a, sid_b) {
            n += 1;
        }
    }
    if n != 0 {
        log::info!("extra relations: {}", n);
    }
    Ok(())
}

fn load_progs(input_dir: &Path, target: &Target) -> anyhow::Result<Vec<Prog>> {
    let dir_iter = read_dir(input_dir)
        .with_context(|| format!("failed to read_dir: {}", input_dir.display()))?;
    let mut failed = 0;
    let mut progs = Vec::new();
    let mut rng = SmallRng::from_entropy();

    for f in dir_iter.filter_map(|f| f.ok()) {
        let path = f.path();
        let content =
            read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
        let p = match parse_prog(target, &content) {
            Ok(p) => p,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        progs.push(p);
    }
    log::info!("progs loaded: {}/{}", progs.len(), progs.len() + failed);
    progs.shuffle(&mut rng);
    Ok(progs)
}

fn load_crash_whitelist(path: &Path) -> anyhow::Result<HashSet<String>> {
    let content = read_to_string(path)
        .with_context(|| format!("failed to read crash titles file {}", path.display()))?;
    let mut titles = HashSet::new();
    for t in content.lines().map(|l| l.trim()) {
        if t.is_empty() {
            continue;
        }
        titles.insert(t.to_string());
    }
    if !titles.is_empty() {
        log::info!("crash whitelist: {} entries", titles.len());
    }
    Ok(titles)
}

fn setup_fuzzer_shm(fuzzer_id: u64, config: &mut Config) -> anyhow::Result<()> {
    let in_shm_id = format!("healer-in_shm-{}-{}", fuzzer_id, std::process::id());
    let out_shm_id = format!("healer-out_shm_{}-{}", fuzzer_id, std::process::id());
    let in_shm = create_shm(&in_shm_id, IN_SHM_SZ).context("failed to create input shm")?;
    let out_shm = create_shm(&out_shm_id, OUT_SHM_SZ).context("failed to create outpout shm")?;
    config.qemu_config.shmids.clear(); // clear old shms
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

#[inline]
fn spawn_syz(
    remote_syz_exec: &Path,
    qemu: &QemuHandle,
    exec: &mut ExecutorHandle,
) -> anyhow::Result<()> {
    let mut ssh = ssh_syz_cmd(remote_syz_exec, qemu);
    ssh.arg("use-ivshm"); // use ivshm mode.
    exec.spawn(ssh).map_err(|e| e.into())
}

fn split_input_progs(progs: Vec<Prog>, job: u64) -> Vec<Vec<Prog>> {
    if progs.is_empty() {
        return Vec::new();
    }
    let job = job as usize;
    let n = progs.len() + job - 1;
    let m = n / job;
    progs.chunks(m).map(|c| c.to_vec()).collect()
}

fn scp_to_vm(p: &Path, qemu: &QemuHandle) -> anyhow::Result<PathBuf> {
    let (vm_ip, vm_port) = qemu.addr().unwrap();
    let (ssh_key, ssh_user) = qemu.ssh().unwrap();
    let f = p.file_name().unwrap();
    let to = PathBuf::from("~").join(f);
    healer_vm::ssh::scp(vm_ip, vm_port, ssh_key, ssh_user, p, &to)
        .context("failed to scp to vm")?;
    Ok(to)
}

#[inline]
fn ssh_syz_cmd(syz: &Path, qemu: &QemuHandle) -> Command {
    let (vm_ip, vm_port) = qemu.addr().unwrap();
    let (ssh_key, ssh_user) = qemu.ssh().unwrap();
    let mut ssh = ssh_basic_cmd(vm_ip, vm_port, ssh_key, ssh_user);
    ssh.arg(syz);
    ssh
}

#[inline]
fn prepare_exec_env(
    config: &mut Config,
    qemu: &mut QemuHandle,
    exec: &mut ExecutorHandle,
) -> anyhow::Result<()> {
    let exec_config = config.exec_config.as_ref().unwrap();
    qemu.boot()
        .with_context(|| format!("failed boot qemu for fuzzer-{}", exec_config.pid))?;
    let remote_exec_path = scp_to_vm(&config.syz_executor(), qemu)?;
    config.remote_exec = Some(remote_exec_path.clone());
    let ssh_syz = ssh_syz_cmd(&remote_exec_path, qemu);
    setup_features(ssh_syz, exec_config.features).context("failed to setup features")?;
    spawn_syz(&remote_exec_path, qemu, exec)
        .with_context(|| format!("failed to spawn executor for fuzzer-{}", exec_config.pid))
}

const HEALER: &str = r"
 ___   ___   ______   ________   __       ______   ______
/__/\ /__/\ /_____/\ /_______/\ /_/\     /_____/\ /_____/\
\::\ \\  \ \\::::_\/_\::: _  \ \\:\ \    \::::_\/_\:::_ \ \
 \::\/_\ .\ \\:\/___/\\::(_)  \ \\:\ \    \:\/___/\\:(_) ) )_
  \:: ___::\ \\::___\/_\:: __  \ \\:\ \____\::___\/_\: __ `\ \
   \: \ \\::\ \\:\____/\\:.\ \  \ \\:\/___/\\:\____/\\ \ `\ \ \
    \__\/ \::\/ \_____\/ \__\/\__\/ \_____\/ \_____\/ \_\/ \_\/
";
