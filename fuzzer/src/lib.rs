#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

use crate::corpus::Corpus;
use crate::exec::{Executor, ExecutorConf};
use crate::feedback::FeedBack;
use crate::fuzzer::Fuzzer;
use crate::guest::{GuestConf, QemuConf, SSHConf};
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;
use core::analyze::static_analyze;
use core::prog::Prog;
use core::target::Target;
use fots::types::Items;
use std::sync::Arc;
use tokio::fs::read;
use tokio::signal::ctrl_c;
use tokio::sync::{broadcast, Barrier};
use tokio::time;

#[macro_use]
pub mod utils;
pub mod corpus;
pub mod exec;
#[allow(dead_code)]
pub mod feedback;
pub mod fuzzer;
pub mod guest;
pub mod report;
pub mod stats;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fots_bin: String,
    pub curpus: Option<String>,
    pub vm_num: usize,

    pub guest: GuestConf,
    pub qemu: Option<QemuConf>,
    pub ssh: Option<SSHConf>,

    pub executor: ExecutorConf,
}

pub async fn fuzz(cfg: Config) {
    let cfg = Arc::new(cfg);

    let (target, candidates) = tokio::join!(load_target(&cfg), load_candidates(&cfg.curpus));

    // shared between multi tasks
    let target = Arc::new(target);
    let candidates = Arc::new(candidates);
    let corpus = Arc::new(Corpus::default());
    let feedback = Arc::new(FeedBack::default());
    let record = Arc::new(TestCaseRecord::new(target.clone()));
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);

    let barrier = Arc::new(Barrier::new(cfg.vm_num + 1));

    for i in 0..cfg.vm_num {
        let cfg = cfg.clone();

        let fuzzer = Fuzzer {
            rt: static_analyze(&target),
            target: target.clone(),
            conf: Default::default(),
            candidates: candidates.clone(),

            corpus: corpus.clone(),
            feedback: feedback.clone(),
            record: record.clone(),

            shutdown: shutdown_tx.subscribe(),
        };
        let barrier = barrier.clone();

        tokio::spawn(async move {
            let mut executor = Executor::new(&cfg);
            println!("Booting kernel, executor ({})...", i);
            executor.start().await;
            barrier.wait().await;
            fuzzer.fuzz(executor).await;
        });
    }

    barrier.wait().await;
    tokio::spawn(async move {
        ctrl_c().await.expect("failed to listen for event");
        shutdown_tx.send(()).unwrap();
        eprintln!("Stopping, wait for persisting data...");
        while shutdown_tx.receiver_count() != 0 {}
    });

    loop {
        use broadcast::TryRecvError::*;
        match shutdown_rx.try_recv() {
            Ok(_) => return,
            Err(e) => match e {
                Empty => (),
                Closed | Lagged(_) => panic!("Unexpected braodcast receiver state"),
            },
        }
        time::delay_for(time::Duration::new(15, 0)).await;
        println!(
            "Corpus:{} Bnranches:{} Blocks:{} candidates:{}",
            corpus.len().await,
            feedback.branch_len().await,
            feedback.block_len().await,
            candidates.len().await
        );
    }
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
