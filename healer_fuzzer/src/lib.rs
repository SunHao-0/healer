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
    config::Config,
    crash::CrashManager,
    feedback::Feedback,
    fuzzer::{Fuzzer, SharedState, HISTORY_CAPACITY},
    stats::Stats,
    util::{stop_req, stop_soon},
};
use anyhow::Context;
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
    fs::{create_dir_all, read_dir, read_to_string, rename, OpenOptions},
    io::BufWriter,
    os::raw::c_int,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
    sync::Arc,
    thread::{self, sleep},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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
        unix_socks: None,
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

    if !config.output.exists() {
        create_dir_all(&config.output).context("failed to create output directory")?;
    }

    let mut all_progs = Vec::new();
    if let Some(p) = config.input.as_ref() {
        log::info!("loading input progs...");
        all_progs = load_progs(p, &target).context("failed to load input progs")?;
    }
    let corpus = config.output.join("corpus");
    if corpus.exists() {
        // resume
        log::info!("loading corpus progs...");
        let corpus_progs = load_progs(&corpus, &target).context("failed to load corpus progs")?;
        all_progs.extend(corpus_progs);
    }
    let mut input_progs = Vec::new();
    if !all_progs.is_empty() {
        log::info!("progs loaded: {}", all_progs.len());
        let mut rng = SmallRng::from_entropy();
        all_progs.shuffle(&mut rng);
        input_progs = split_input_progs(all_progs, config.job);
    }

    let mut known_crashes = HashSet::new();
    if let Some(l) = config.crash_whitelist.as_ref() {
        log::info!("loading crash whitelist...");
        known_crashes = load_crash_whitelist(l).context("failed to load crash whitelist")?;
        if !known_crashes.is_empty() {
            log::info!("whitelist: {}", known_crashes.len());
        }
    }
    let whitelist_file = config.output.join("whitelist");
    if whitelist_file.is_file() {
        log::info!("loading old whitelist...");
        let old_known_crashes =
            load_crash_whitelist(&whitelist_file).context("failed to load crash whitelist")?;
        if !old_known_crashes.is_empty() {
            log::info!("whitelist: {}", old_known_crashes.len());
        }
        known_crashes.extend(old_known_crashes);
    }
    let crash_dir = config.output.join("crashes");
    if crash_dir.is_dir() {
        // resume
        log::info!("collecting reproed crashes...");
        let reproed = collect_reproed_crashes(&crash_dir).context("failed to scan old crashes")?;
        if !reproed.is_empty() {
            log::info!("reproed crashes: {}", reproed.len());
            known_crashes.extend(reproed);
        }
        // move to old
        if let Some(old_dir) = maybe_mv_to_old(&crash_dir)? {
            log::info!("old crashes moved to: {}", old_dir.display());
            create_dir_all(&crash_dir).context("failed to create crashes dir")?;
        }
    }
    if !known_crashes.is_empty() {
        log::info!("total known crashes: {}", known_crashes.len());
        dump_crash_whitelist(&config.output, &known_crashes)?;
    }
    let crash = CrashManager::with_whitelist(known_crashes, config.output.clone());

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
    if config.use_unix_sock {
        setup_unix_sock(0, &mut config);
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
    spawn_syz(&remote_exec_path, &mut qemu, &mut executor, &config)
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

        let handle = thread::spawn(move || {
            fuzzer_config.exec_config.as_mut().unwrap().pid = id;
            if use_shm {
                setup_fuzzer_shm(id, &mut fuzzer_config)
                    .with_context(|| format!("failed to setup shm for fuzzer-{}", id))?;
            }
            if fuzzer_config.use_unix_sock {
                setup_unix_sock(id, &mut fuzzer_config);
            }
            fuzzer_config.fixup();

            let mut qemu = QemuHandle::with_config(fuzzer_config.qemu_config.clone());
            let exec_config = fuzzer_config.exec_config.take().unwrap();
            fuzzer_config.exec_config = Some(exec_config.clone());
            let mut executor = ExecutorHandle::with_config(exec_config);
            prepare_exec_env(&mut fuzzer_config, &mut qemu, &mut executor)?;
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

        fuzzers.push(handle);

        if id != config1.job - 1 {
            sleep(Duration::from_secs(5)); // slow down
        }

        if stop_soon() {
            break;
        }
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
    let mut progs = Vec::new();

    for f in dir_iter.filter_map(|f| f.ok()) {
        let path = f.path();
        let content = match read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue, // skip
        };
        if let Ok(p) = parse_prog(target, &content) {
            progs.push(p);
        }
    }
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
    Ok(titles)
}

fn collect_reproed_crashes(dir: &Path) -> anyhow::Result<HashSet<String>> {
    let mut reproed = HashSet::new();
    for entry in read_dir(dir)
        .context("failed to read old crashes dir")?
        .filter_map(|e| e.ok())
    {
        let entry = entry.path();
        if !entry.is_dir() {
            continue;
        }
        let meta = entry.join("meta");
        let c_repro = entry.join("repro.c");
        if meta.is_file() && c_repro.is_file() {
            let name = entry.file_name().unwrap();
            reproed.insert(name.to_string_lossy().trim().to_string());
        }
    }
    Ok(reproed)
}

fn maybe_mv_to_old(dir: &Path) -> anyhow::Result<Option<PathBuf>> {
    let empty = read_dir(dir)
        .context("failed to read crashes dir")?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .count()
        == 0;
    let mut new_dir = None;

    if !empty {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let new_dir0 = PathBuf::from(format!("{}-{}", dir.display(), now));
        rename(dir, &new_dir0).with_context(|| {
            format!(
                "failed to rename old crashes: {} -> {}",
                dir.display(),
                new_dir0.display()
            )
        })?;
        new_dir = Some(new_dir0);
    }
    Ok(new_dir)
}

fn dump_crash_whitelist(out: &Path, crashes: &HashSet<String>) -> anyhow::Result<()> {
    use std::io::Write;

    let f = out.join("whitelist");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&f)
        .with_context(|| format!("failed to open: {}", f.display()))?;
    let mut w = BufWriter::new(f);
    for crash in crashes {
        writeln!(w, "{}", crash).with_context(|| format!("failed to write crash: {}", crash))?;
    }
    Ok(())
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
    qemu: &mut QemuHandle,
    exec: &mut ExecutorHandle,
    config: &Config,
) -> anyhow::Result<()> {
    if config.use_unix_sock {
        let cmd = ssh_bg_syz_cmd(remote_syz_exec, qemu);
        let exec_config = config.exec_config.as_ref().unwrap();
        // TODO fix this
        let (stdin, stdout, stderr) = exec_config.unix_socks.as_ref().unwrap();
        let stdin = qemu.char_dev_sock(stdin).unwrap();
        let stdout = qemu.char_dev_sock(stdout).unwrap();
        let stderr = qemu.char_dev_sock(stderr);
        exec.spawn_with_channel(cmd, (stdin, stdout, stderr))
            .map_err(|e| e.into())
    } else {
        let mut ssh = ssh_syz_cmd(remote_syz_exec, qemu);
        ssh.arg("use-ivshm"); // use ivshm mode.
        exec.spawn(ssh).map_err(|e| e.into())
    }
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
fn ssh_bg_syz_cmd(syz: &Path, qemu: &QemuHandle) -> Command {
    let (vm_ip, vm_port) = qemu.addr().unwrap();
    let (ssh_key, ssh_user) = qemu.ssh().unwrap();
    let mut ssh = ssh_basic_cmd(vm_ip, vm_port, ssh_key, ssh_user);
    let cmd = format!("nohup {} use-ivshm use-unix-socks&", syz.display());
    ssh.arg(cmd);
    ssh
}

fn setup_unix_sock(fuzzer_id: u64, config: &mut Config) {
    // sync with executor
    const PORT_STDIN: u8 = 30;
    const PORT_STDOUT: u8 = 29;
    const PORT_STDERR: u8 = 28;

    let pid = std::process::id();
    let stdin = format!("/tmp/healer-exec-stdin-{}-{}", pid, fuzzer_id);
    let stdout = format!("/tmp/healer-exec-stdout-{}-{}", pid, fuzzer_id);
    let stderr = format!("/tmp/healer-exec-stderr-{}-{}", pid, fuzzer_id);
    config.exec_config.as_mut().unwrap().unix_socks =
        Some((stdin.clone(), stdout.clone(), stderr.clone()));
    config.qemu_config.serial_ports.push((stdin, PORT_STDIN));
    config.qemu_config.serial_ports.push((stdout, PORT_STDOUT));
    config.qemu_config.serial_ports.push((stderr, PORT_STDERR));
    config.qemu_config.serial_ports.shrink_to_fit();
}

#[inline]
fn prepare_exec_env(
    config: &mut Config,
    qemu: &mut QemuHandle,
    exec: &mut ExecutorHandle,
) -> anyhow::Result<()> {
    let exec_config = config.exec_config.as_ref().unwrap();
    retry_exec(|| qemu.boot())
        .with_context(|| format!("failed boot qemu for fuzzer-{}", exec_config.pid))?;
    let remote_exec_path = retry_exec(|| scp_to_vm(&config.syz_executor(), qemu))?;
    config.remote_exec = Some(remote_exec_path.clone());
    retry_exec(|| {
        let ssh_syz = ssh_syz_cmd(&remote_exec_path, qemu);
        setup_features(ssh_syz, exec_config.features)
    })
    .context("failed to setup features")?;
    let r = spawn_syz(&remote_exec_path, qemu, exec, config);
    if r.is_err() {
        retry_exec(|| exec.respawn())
            .with_context(|| format!("failed to spawn executor for fuzzer-{}", exec_config.pid))?
    }
    Ok(())
}

pub(crate) fn retry_exec<T, E>(mut f: impl FnMut() -> Result<T, E>) -> Result<T, E> {
    let mut tried = 0;
    let max = 3;

    loop {
        match f() {
            Ok(r) => return Ok(r),
            Err(e) => {
                if tried < max && !stop_soon() {
                    sleep(Duration::from_secs(10)); // wait
                    tried += 1;
                } else {
                    return Err(e);
                }
            }
        }
    }
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
