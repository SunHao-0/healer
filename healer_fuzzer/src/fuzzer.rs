use crate::{
    config::Config, crash::Crash, feedback::Feedback, fuzzer_log::set_fuzzer_id, stats::Stats,
    util::stop_soon,
};
use anyhow::Context;
use healer_core::{
    corpus::CorpusWrapper, gen::gen_prog, mutation::mutate, prog::Prog, relation::RelationWrapper,
    target::Target, RngType,
};
use healer_vm::qemu::QemuHandle;
use std::{collections::VecDeque, sync::Arc};
use syz_wrapper::exec::{ExecError, ExecOpt, ExecutorHandle};

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
    pub id: u64,
    pub rng: RngType,
    pub executor: ExecutorHandle,
    pub qemu: QemuHandle,
    pub run_history: VecDeque<(Prog, ExecOpt)>,
    pub config: Config,
}

pub const HISTORY_CAPACITY: usize = 1024;

impl Fuzzer {
    pub fn fuzz_loop(&mut self, progs: Vec<Prog>) -> anyhow::Result<()> {
        const GENERATE_PERIOD: u64 = 100;

        set_fuzzer_id(self.id);
        for prog in progs {
            self.execute_one(prog)
                .context("failed to execute input prog")?;
        }

        for i in 0_u64.. {
            // TODO update period based on gaining
            if self.shared_state.corpus.is_empty() || i % GENERATE_PERIOD == 0 {
                let p = gen_prog(
                    &self.shared_state.target,
                    &self.shared_state.relation,
                    &mut self.rng,
                );
                self.execute_one(p)
                    .context("failed to execute generated prog")?;
            } else {
                let mut p = self.shared_state.corpus.select_one(&mut self.rng).unwrap();
                mutate(
                    &self.shared_state.target,
                    &self.shared_state.relation,
                    &self.shared_state.corpus,
                    &mut self.rng,
                    &mut p,
                );
                self.execute_one(p)
                    .context("failed to execute mutated prog")?;
            }

            if stop_soon() {
                break;
            }
        }

        Ok(())
    }

    pub fn execute_one(&mut self, p: Prog) -> anyhow::Result<bool> {
        let opt = ExecOpt::new();
        self.record_execution(&p, &opt);
        let ret = self
            .executor
            .execute_one(&self.shared_state.target, &p, &opt);

        match ret {
            Ok(_prog_info) => {
                todo!()
                // let new = self
                //     .check_new_cov(&p, &prog_info)
                //     .context("failed to handle prog info")?;
                // if !new.is_empty() {
                // }
            }
            Err(e) => {
                if let Some(crash) = self.check_vm(&p, e) {
                    self.handle_crash(crash).context("failed to handle crash")?;
                    return Ok(true);
                }
            }
        }
        todo!()
    }

    fn check_vm(&mut self, p: &Prog, e: ExecError) -> Option<Vec<u8>> {
        fuzzer_warn!("failed to exec prog: {}", e);
        if matches!(
            e,
            ExecError::Io(_) | ExecError::Signal | ExecError::UnexpectedExitStatus(_)
        ) && !self.qemu.is_alive()
        {
            fuzzer_warn!(
                "kernel crashed, maybe caused by: {}",
                p.display(&self.shared_state.target)
            );
            let log = self.qemu.collect_crash_log().unwrap();
            Some(log)
        } else {
            None
        }
    }

    fn handle_crash(&mut self, _crash_log: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    fn record_execution(&mut self, p: &Prog, opt: &ExecOpt) {
        if self.run_history.len() >= HISTORY_CAPACITY {
            self.run_history.pop_front();
        }
        self.run_history.push_back((p.clone(), opt.clone()))
    }
}
