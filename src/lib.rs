#[macro_use]
extern crate lazy_static;

#[macro_use]
pub mod utils;
pub mod exec;
pub mod fuzz;
pub mod gen;
pub mod model;
pub mod targets;

use exec::SshConf;
use fuzz::{
    fuzzer::{Fuzzer, Mode},
    queue::Queue,
    relation::Relation,
    stats::{bench, Stats},
};
use rustc_hash::{FxHashMap, FxHashSet};
use targets::Target;

use crate::exec::{ExecConf, QemuConf};

use std::{
    collections::VecDeque,
    fs::create_dir,
    io::ErrorKind,
    path::PathBuf,
    process::exit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Barrier, Mutex, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Config {
    pub target: String,
    pub kernel_obj: Option<PathBuf>,
    pub kernel_src: Option<PathBuf>,
    pub jobs: u64,
    pub relations: Option<PathBuf>,
    pub symbolizer: PathBuf,
    pub qemu_conf: QemuConf,
    pub exec_conf: ExecConf,
    pub ssh_conf: SshConf,
    pub work_dir: PathBuf,
}

pub fn start(conf: Config) {
    let max_cov = Arc::new(RwLock::new(FxHashSet::default()));
    let calibrated_cov = Arc::new(RwLock::new(FxHashSet::default()));
    let crashes = Arc::new(Mutex::new(FxHashMap::default()));
    let raw_crashes = Arc::new(Mutex::new(VecDeque::with_capacity(1024)));
    let stats = Arc::new(Stats::new());
    let stop = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(conf.jobs as usize + 1));
    let mut fuzzers = Vec::new();

    if let Err(e) = create_dir(&conf.work_dir) {
        if e.kind() == ErrorKind::AlreadyExists {
            let crash_dir = conf.work_dir.join("crashes");
            if crash_dir.exists() {
                log::warn!(
                    "Existing crash data ({}) may be overwritten",
                    crash_dir.display()
                );
            }
        } else {
            log::error!(
                "Failed to create work directory {}: {}",
                conf.work_dir.display(),
                e
            );
            exit(1);
        }
    }

    log::info!("Loading target {}...", conf.target);
    let target = Target::new(&conf.target).unwrap_or_else(|| {
        // preloading.
        log::error!("Target {} dose not exist", conf.target);
        exit(1);
    });

    let relations_file = if let Some(f) = conf.relations.as_ref() {
        f.clone()
    } else {
        conf.work_dir.join("relations")
    };
    let relations = Relation::load(&target, &relations_file).unwrap_or_else(|e| {
        log::error!(
            "Failed to load relations '{}': {}",
            relations_file.display(),
            e
        );
        exit(1);
    });
    let relations = Arc::new(relations);
    log::info!("Initial relations: {}.", relations.len());

    log::info!("Boot {} {} on qemu...", conf.jobs, conf.target);
    let start = Instant::now();
    for id in 0..conf.jobs {
        let max_cov = Arc::clone(&max_cov);
        let calibrated_cov = Arc::clone(&calibrated_cov);
        let relations = Arc::clone(&relations);
        let crashes = Arc::clone(&crashes);
        let raw_crashes = Arc::clone(&raw_crashes);
        let stats = Arc::clone(&stats);
        let barrier = Arc::clone(&barrier);
        let stop = Arc::clone(&stop);
        let conf = conf.clone();
        let symbolizer = conf.symbolizer.clone();

        let handle = thread::spawn(move || {
            let conf = conf.clone();
            let target = Target::new(&conf.target).unwrap();
            let exec_handle =
                match exec::spawn_in_qemu(conf.exec_conf, conf.qemu_conf, conf.ssh_conf, id) {
                    Ok(handle) => handle,
                    Err(e) => {
                        log::error!("failed to boot: {}", e);
                        exit(1)
                    }
                };
            barrier.wait();

            let mut queue = match Queue::with_workdir(id as usize, conf.work_dir.clone()) {
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

            let mut fuzzer = Fuzzer {
                symbolizer,
                max_cov,
                calibrated_cov,
                relations,
                crashes,
                raw_crashes,
                stats,
                id,
                target,
                local_vals: FxHashMap::default(),
                queue,
                exec_handle,
                run_history: VecDeque::with_capacity(128),
                mode: Mode::Sampling,
                mut_gaining: 0,
                gen_gaining: 0,
                cycle_len: 128,
                max_cycle_len: 1024,
                work_dir: conf.work_dir,
                kernel_obj: conf.kernel_obj,
                kernel_src: conf.kernel_src,
                last_reboot: Instant::now(),
                stop,
            };
            fuzzer.fuzz();
        });
        fuzzers.push(handle);
    }

    barrier.wait();

    ctrlc::set_handler(move || {
        stop.store(true, Ordering::Relaxed);
        println!("Waiting fuzzers to exit...");
        while !fuzzers.is_empty() {
            let f = fuzzers.pop().unwrap();
            f.join().unwrap();
        }
        println!("Ok, have a nice day. *-*");
        exit(0)
    })
    .unwrap();

    log::info!("Boot finished, cost {}s", start.elapsed().as_secs());
    log::info!("Let the fuzz begin");
    bench(Duration::new(10, 0), conf.work_dir, stats);
}
