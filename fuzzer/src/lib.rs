#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

use crate::corpus::Corpus;
use crate::exec::{startup, ExecHandle, Executor};
use crate::feedback::FeedBack;
use crate::fuzzer::Fuzzer;
use crate::qemu::{boot, Qemu};
use crate::ssh::SshConfig;
use crate::utils::process::Handle;
use crate::utils::queue::CQueue;
use crate::utils::split::Split;
use core::analyze::static_analyze;
use core::prog::Prog;
use core::target::Target;
use fots::types::Items;
use std::sync::Arc;
use tokio::fs::read;
use tokio::sync::Barrier;
use tokio::time;

pub mod corpus;
pub mod exec;
pub mod feedback;
pub mod fuzzer;
pub mod qemu;
pub mod ssh;
pub mod utils;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fots_bin: String,
    pub curpus: Option<String>,
    pub vm_num: usize,
    pub qemu: Qemu,
    pub ssh: SshConfig,
    pub executor: Executor,
}

pub async fn fuzz(cfg: Config) {
    let cfg = Arc::new(cfg);

    let (mut targets, candidates) = tokio::join!(load_target(&cfg), load_candidates(&cfg.curpus));
    // println!("Target Groups:{}",targets.groups.len());

    let candidates = Arc::new(candidates);
    let corpus = Arc::new(Corpus::default());
    let feedback = Arc::new(FeedBack::default());

    let barrier = Arc::new(Barrier::new(cfg.vm_num + 1));
    // let mut handles = Vec::new();

    for i in 0..cfg.vm_num {
        let target = targets.pop().unwrap();
        let cfg = cfg.clone();
        println!("Fuzzer{}: Groups {}", i, target.groups.len());

        let fuzzer = Arc::new(Fuzzer {
            rt: static_analyze(&target),
            conf: Default::default(),
            corpus: corpus.clone(),
            feedback: feedback.clone(),
            candidates: candidates.clone(),

            target,
        });
        let barrier = barrier.clone();

        tokio::spawn(async move {
            let (qemu, executor) = init(cfg.as_ref()).await;
            barrier.wait().await;
            fuzzer.as_ref().fuzz(qemu, executor).await;
        });
    }

    barrier.wait().await;
    loop {
        time::delay_for(time::Duration::new(15, 0)).await;
        println!(
            "Corpus:{} Feedback:{} candidates:{}",
            corpus.len().await,
            feedback.len().await,
            candidates.len().await
        );
    }
}

pub async fn init(cfg: &Config) -> (Handle, ExecHandle) {
    let (qemu, port) = boot(cfg).await;
    let executor = startup(cfg, port).await;
    (qemu, executor)
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

async fn load_target(cfg: &Config) -> Vec<Target> {
    let mut items = Items::load(&read(&cfg.fots_bin).await.unwrap()).unwrap();
    split(&mut items, cfg.vm_num)
}

fn split(items: &mut Items, n: usize) -> Vec<Target> {
    assert!(items.groups.len() > n);

    let mut result = Vec::new();
    let total = items.groups.len();

    for n in Split::new(total, n) {
        let sub_groups = items.groups.drain(items.groups.len() - n..);
        let target = Target::new(Items {
            types: items.types.clone(),
            groups: sub_groups.collect(),
            rules: vec![],
        });
        result.push(target);
    }
    result
}
