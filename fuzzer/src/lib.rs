#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

use crate::corpus::Corpus;
use crate::exec::{Executor, ExecutorConf};
use crate::feedback::FeedBack;
use crate::fuzzer::Fuzzer;
use crate::guest::{GuestConf, QemuConf, SSHConf};
use crate::mail::MailConf;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;

use circular_queue::CircularQueue;
use core::analyze::static_analyze;
use core::prog::Prog;
use core::target::Target;
use fots::types::Items;
use std::process;
use std::sync::Arc;
use tokio::fs::{create_dir_all, read};
use tokio::signal::ctrl_c;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, Barrier};

#[macro_use]
pub mod utils;
pub mod corpus;
pub mod exec;
#[allow(dead_code)]
pub mod feedback;
pub mod fuzzer;
pub mod guest;
pub mod mail;
pub mod report;
pub mod stats;

use crate::stats::SamplerConf;
use stats::StatSource;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fots_bin: String,
    pub curpus: Option<String>,
    pub vm_num: usize,

    pub guest: GuestConf,
    pub qemu: Option<QemuConf>,
    pub ssh: Option<SSHConf>,

    pub executor: ExecutorConf,

    pub mail: Option<MailConf>,
    pub sampler: Option<SamplerConf>,
}

pub async fn fuzz(cfg: Config) {
    let cfg = Arc::new(cfg);
    let work_dir = std::env::var("HEALER_WORK_DIR").unwrap_or_else(|_| String::from("."));
    let (target, candidates) = tokio::join!(load_target(&cfg), load_candidates(&cfg.curpus));
    info!("Corpus: {}", candidates.len().await);

    if let Some(mail_conf) = cfg.mail.as_ref() {
        mail::init(mail_conf);
        info!("Email report to: {:?}", mail_conf.receivers);
    }

    // shared between multi tasks
    let target = Arc::new(target);
    let candidates = Arc::new(candidates);
    let corpus = Arc::new(Corpus::default());
    let feedback = Arc::new(FeedBack::default());
    let record = Arc::new(TestCaseRecord::new(target.clone(), work_dir.clone()));
    let rt = Arc::new(Mutex::new(static_analyze(&target)));

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

    let barrier = Arc::new(Barrier::new(cfg.vm_num + 1));

    info!(
        "Booting {} {}/{} on {} ...",
        cfg.vm_num, cfg.guest.os, cfg.guest.arch, cfg.guest.platform
    );
    let now = std::time::Instant::now();

    for _ in 0..cfg.vm_num {
        let cfg = cfg.clone();

        let fuzzer = Fuzzer {
            rt: rt.clone(),
            target: target.clone(),
            conf: Default::default(),
            candidates: candidates.clone(),

            corpus: corpus.clone(),
            feedback: feedback.clone(),
            record: record.clone(),

            shutdown: shutdown_tx.subscribe(),
            work_dir: work_dir.clone(),
        };

        let barrier = barrier.clone();

        tokio::spawn(async move {
            let mut executor = Executor::new(&cfg);
            executor.start().await;
            barrier.wait().await;
            fuzzer.fuzz(executor).await;
        });
    }

    barrier.wait().await;
    info!("Boot finished, cost {}s.", now.elapsed().as_secs());

    tokio::spawn(async move {
        ctrl_c().await.expect("failed to listen for event");
        shutdown_tx.send(()).unwrap();
        warn!("Stopping, persisting data...");
        while shutdown_tx.receiver_count() != 0 {}
    });
    let mut sampler = stats::Sampler {
        source: StatSource {
            corpus,
            feedback,
            candidates,
            record,
        },
        stats: CircularQueue::with_capacity(1024),
        shutdown: shutdown_rx,
        work_dir,
    };
    sampler.sample(&cfg.sampler).await;
}

async fn load_candidates(path: &Option<String>) -> CQueue<Prog> {
    if let Some(path) = path.as_ref() {
        let data = read(path).await.unwrap();
        let progs: Vec<Prog> = bincode::deserialize(&data).unwrap();

        CQueue::from(progs)
    } else {
        CQueue::default()
    }
}

async fn load_target(cfg: &Config) -> Target {
    let items = Items::load(&read(&cfg.fots_bin).await.unwrap()).unwrap();
    // split(&mut items, cfg.vm_num)
    Target::from(items)
}

pub async fn prepare_env() {
    pretty_env_logger::init_timed();
    let pid = process::id();
    std::env::set_var("HEALER_FUZZER_PID", format!("{}", pid));
    info!("Pid: {}", pid);

    let work_dir = std::env::var("HEALER_WORK_DIR").unwrap_or_else(|_| String::from("."));
    std::env::set_var("HEALER_WORK_DIR", &work_dir);
    info!("Work-dir: {}", work_dir);

    use tokio::io::ErrorKind::*;

    if let Err(e) = create_dir_all(format!("{}/crashes", work_dir)).await {
        if e.kind() != AlreadyExists {
            exits!(exitcode::IOERR, "Fail to create crash dir: {}", e);
        }
    }
}

// fn split(items: &mut Items, n: usize) -> Vec<Target> {
//     assert!(items.groups.len() > n);
//
//     let mut result = Vec::new();
//     let total = items.groups.len();
//
//     for n in Split::new(total, n) {
//         let sub_groups = items.groups.drain(items.groups.len() - n..);
//         let target = Target::from(Items {
//             types: items.types.clone(),
//             groups: sub_groups.collect(),
//             rules: vec![],
//         });
//         result.push(target);
//     }
//     result
// }
