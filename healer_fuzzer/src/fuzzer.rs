use crate::{config::Config, crash::Crash, feedback::Feedback, stats::Stats};
use healer_core::{
    corpus::CorpusWrapper, prog::Prog, relation::RelationWrapper, target::Target, RngType,
};
use healer_vm::qemu::QemuHandle;
use std::{collections::VecDeque, sync::Arc};
use syz_wrapper::exec::{ExecOpt, ExecutorHandle};

pub struct SharedState {
    pub target: Arc<Target>,
    pub relation: Arc<RelationWrapper>,
    pub corpus: Arc<CorpusWrapper>,
    pub stats: Arc<Stats>,
    pub feedback: Arc<Feedback>,
    pub crash: Arc<Crash>,
}

impl Clone for SharedState {
    fn clone(&self) -> Self {
        Self {
            target: Arc::clone(&self.target),
            relation: Arc::clone(&self.relation),
            corpus: Arc::clone(&self.corpus),
            stats: Arc::clone(&self.stats),
            feedback: Arc::clone(&self.feedback),
            crash: Arc::clone(&self.crash),
        }
    }
}

pub struct Fuzzer {
    pub shared_state: SharedState,

    // local
    pub id: usize,
    pub rng: RngType,
    pub executor: ExecutorHandle,
    pub qemu: QemuHandle,
    pub run_history: VecDeque<(Prog, ExecOpt)>,
    pub config: Config,
}

pub const HISTORY_CAPACITY: usize = 1024;

impl Fuzzer {
    pub fn fuzz_loop(&mut self, _progs: Vec<Prog>) -> anyhow::Result<()> {
        todo!()
    }
}
