#![allow(clippy::collapsible_else_if, clippy::missing_safety_doc)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
pub mod utils;
pub mod exec;
pub mod fuzz;
pub mod gen;
pub mod model;
pub mod targets;
pub mod vm;

use crate::targets::Target;
use crate::{
    fuzz::{
        features,
        fuzzer::Fuzzer,
        queue::Queue,
        relation::Relation,
        stats::{bench, Stats},
    },
    utils::notify_stop,
};

use std::{
    collections::VecDeque,
    fs::{create_dir, read_to_string},
    io::ErrorKind,
    os::raw::c_int,
    path::PathBuf,
    process::exit,
    sync::{Arc, Barrier, Mutex, RwLock},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use exec::syz::SyzExecConfig;
use rustc_hash::{FxHashMap, FxHashSet};
use vm::qemu::QemuConfig;

#[derive(Debug, Clone)]
pub struct Config {
    pub target: String,
    pub kernel_obj_dir: Option<PathBuf>,
    pub kernel_src_dir: Option<PathBuf>,
    pub syz_bin_dir: PathBuf,
    pub out_dir: PathBuf,
    pub relations: Option<PathBuf>,
    pub jobs: u64,
    pub skip_repro: bool,
    pub disabled_calls: Option<PathBuf>,
    pub white_list: Option<PathBuf>,
    pub enable_relation_detect: bool,
    pub qemu_conf: QemuConfig,
    pub exec_conf: SyzExecConfig,
}

impl Config {
    pub fn check(&self) -> Result<(), String> {
        let supported = targets::sys_json::supported();
        if !supported.contains(&self.target) {
            return Err(format!(
                "unspported target: {}.({:?} are supported)",
                self.target, supported
            ));
        }
        if let Some(dir) = self.kernel_obj_dir.as_ref() {
            if !dir.is_dir() {
                return Err(format!(
                    "bad kernel object file directory '{}'.",
                    dir.display()
                ));
            }
        }
        if let Some(dir) = self.kernel_src_dir.as_ref() {
            if !dir.is_dir() {
                return Err(format!(
                    "bad kernel srouce files directory '{}'.",
                    dir.display()
                ));
            }
        }
        if let Some(f) = self.disabled_calls.as_ref() {
            if !f.is_file() {
                return Err(format!("bad disabled system calls file: {}", f.display()));
            }
        }
        if let Some(f) = self.white_list.as_ref() {
            if !f.is_file() {
                return Err(format!("bad white list file: {}", f.display()));
            }
        }
        if !self.syz_bin_dir.is_dir() {
            return Err(format!(
                "bad syzkaller binary files directory '{}'.",
                self.syz_bin_dir.display()
            ));
        }

        let target_bins = vec!["syz-executor", "syz-execprog", "syz-fuzzer"];
        let dir = self.syz_bin_dir.join(self.target.replace("/", "_"));
        for bin in &target_bins {
            let f = dir.join(bin);
            if !f.is_file() {
                return Err(format!(
                    "missing executable file {} in {}",
                    bin,
                    dir.display()
                ));
            }
        }
        let symbolize = self.syz_bin_dir.join("syz-symbolize");
        if !symbolize.is_file() {
            return Err(format!(
                "missing executable file syz-symbolize in {}",
                self.syz_bin_dir.display()
            ));
        }
        self.qemu_conf
            .check()
            .map_err(|e| format!("qemu config: {}", e))
    }
}

pub fn start(conf: Config) {
    if let Err(e) = conf.check() {
        log::error!("config error: {}", e);
        exit(1);
    }

    let max_cov = Arc::new(RwLock::new(FxHashSet::default()));
    let calibrated_cov = Arc::new(RwLock::new(FxHashSet::default()));
    let crashes = Arc::new(Mutex::new(FxHashMap::default()));
    let repros = Arc::new(Mutex::new(FxHashMap::default()));
    let reproducing = Arc::new(Mutex::new(FxHashSet::default()));
    let raw_crashes = Arc::new(Mutex::new(VecDeque::with_capacity(1024)));
    let stats = Arc::new(Stats::new());
    let barrier = Arc::new(Barrier::new(conf.jobs as usize + 1));
    let mut fuzzers = Vec::new();
    let mut white_list = FxHashSet::default();
    if let Some(f) = conf.white_list.as_ref() {
        let l = read_to_string(f).unwrap_or_else(|e| {
            log::error!("failed to load white list '{}': {}", f.display(), e);
            exit(1)
        });
        white_list = l
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.trim().to_string())
            .collect();
    }
    let white_list = Arc::new(white_list);

    if let Err(e) = create_dir(&conf.out_dir) {
        if e.kind() == ErrorKind::AlreadyExists {
            let crash_dir = conf.out_dir.join("crashes");
            if crash_dir.exists() {
                log::warn!(
                    "existing crash data ({}) may be overwritten",
                    crash_dir.display()
                );
            }
        } else {
            log::error!(
                "failed to create output directory {}: {}",
                conf.out_dir.display(),
                e
            );
            exit(1);
        }
    }

    println!("{}", HEALER);

    log::info!("Loading target {}...", conf.target);
    let mut disabled_calls = FxHashSet::default();
    if let Some(f) = conf.disabled_calls.as_ref() {
        let calls = read_to_string(f).unwrap_or_else(|e| {
            log::error!(
                "failed to read disabled system calls file '{}': {}",
                f.display(),
                e
            );
            exit(1)
        });
        disabled_calls = calls
            .lines()
            .filter(|&l| !l.is_empty())
            .map(|c| c.trim().to_string())
            .collect();
    }
    let target = Target::new(&conf.target, &disabled_calls).unwrap_or_else(|e| {
        // preloading.
        log::error!("failed to load target '{}': {}", conf.target, e);
        exit(1)
    });
    log::info!("Revision: {}", &target.revision[0..12]);
    log::info!(
        "Syscalls: {}/{}",
        target.syscalls.len(),
        target.all_syscalls.len()
    );

    let relations_file = if let Some(f) = conf.relations.as_ref() {
        f.clone()
    } else {
        conf.out_dir.join("relations")
    };
    let relations = Relation::load(&target, &relations_file).unwrap_or_else(|e| {
        log::error!(
            "failed to load relations '{}': {}",
            relations_file.display(),
            e
        );
        exit(1);
    });
    let relations = Arc::new(relations);
    log::info!("Initial relations: {}", relations.len());

    log::info!("Booting {} {} on qemu...", conf.jobs, conf.target);
    let start = Instant::now();
    for id in 0..conf.jobs {
        let max_cov = Arc::clone(&max_cov);
        let calibrated_cov = Arc::clone(&calibrated_cov);
        let relations = Arc::clone(&relations);
        let crashes = Arc::clone(&crashes);
        let white_list = Arc::clone(&white_list);
        let reproducing = Arc::clone(&reproducing);
        let repros = Arc::clone(&repros);
        let raw_crashes = Arc::clone(&raw_crashes);
        let stats = Arc::clone(&stats);
        let barrier = Arc::clone(&barrier);
        let conf = conf.clone();
        let disabled_calls = disabled_calls.clone();

        let handle = thread::spawn(move || {
            let conf = conf.clone();
            let target = Target::new(&conf.target, &disabled_calls).unwrap();
            let mut queue = match Queue::with_outdir(id as usize, conf.out_dir.clone()) {
                Ok(q) => q,
                Err(e) => {
                    log::error!("failed to initialize queue-{}: {}", id, e);
                    exit(1)
                }
            };
            if id == 0 {
                // only record queue-0's stats.
                queue.set_stats(Arc::clone(&stats));
            }

            let mut exec_handle =
                match exec::spawn_syz_in_qemu(conf.exec_conf.clone(), conf.qemu_conf.clone(), id) {
                    Ok(handle) => handle,
                    Err(e) => {
                        log::error!("failed to boot: {}", e);
                        exit(1)
                    }
                };
            let features = features::check(&mut exec_handle, id == 0);
            barrier.wait();

            let mut fuzzer = Fuzzer {
                max_cov,
                calibrated_cov,
                relations,
                crashes,
                white_list,
                repros,
                reproducing,
                raw_crashes,
                stats,
                id,
                target,
                conf,
                queue,
                exec_handle,
                run_history: VecDeque::with_capacity(128),
                mut_gaining: 0,
                gen_gaining: 0,
                features,
                cycle_len: 128,
                last_reboot: Instant::now(),
            };
            fuzzer.fuzz();
        });
        fuzzers.push(handle);
    }

    barrier.wait();

    setup_signal_handler(fuzzers);

    log::info!("Boot finished, cost {}s", start.elapsed().as_secs());
    log::info!("Let the fuzz begin");
    bench(Duration::new(10, 0), conf.out_dir, stats);
}

fn setup_signal_handler(mut fuzzers: Vec<JoinHandle<()>>) {
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
        notify_stop();
        while !fuzzers.is_empty() {
            let f = fuzzers.pop().unwrap();
            let _ = f.join();
        }
    });
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
