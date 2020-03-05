use crate::corpus::Corpus;
use crate::exec::Executor;
use crate::feedback::{Block, Branch, FeedBack};
use crate::guest::Crash;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;
use core::analyze::RTable;
use core::c::to_script;
use core::gen::gen;
use core::minimize::minimize;
use core::prog::Prog;
use core::target::Target;
use executor::{ExecResult, Reason};
use fots::types::GroupId;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::fs::write;
use tokio::sync::broadcast;

pub struct Fuzzer {
    pub target: Arc<Target>,
    pub rt: HashMap<GroupId, RTable>,
    pub conf: core::gen::Config,
    pub work_dir: String,

    pub corpus: Arc<Corpus>,
    pub feedback: Arc<FeedBack>,
    pub candidates: Arc<CQueue<Prog>>,
    pub record: Arc<TestCaseRecord>,
    pub shutdown: broadcast::Receiver<()>,
}

impl Fuzzer {
    pub async fn fuzz(mut self, mut executor: Executor) {
        use broadcast::TryRecvError::*;
        loop {
            match self.shutdown.try_recv() {
                Ok(_) => {
                    self.peresist().await;
                    return;
                }
                Err(e) => match e {
                    Empty => (),
                    Closed | Lagged(_) => panic!("Unexpected braodcast receiver state"),
                },
            }

            let p = self.get_prog().await;
            match executor.exec(&p).await {
                Ok(exec_result) => match exec_result {
                    ExecResult::Ok(raw_branches) => {
                        self.feedback_analyze(p, raw_branches, &mut executor).await
                    }
                    ExecResult::Failed(reason) => self.failed_analyze(p, reason).await,
                },
                Err(crash) => {
                    self.crash_analyze(p, crash.unwrap_or_default(), &mut executor)
                        .await
                }
            }
        }
    }

    async fn peresist(self) {
        let corpus_path = format!("{}/corpus", self.work_dir);
        let corpus = self
            .corpus
            .dump()
            .await
            .unwrap_or_else(|e| exits!(exitcode::DATAERR, "Fail to dump corpus: {}", e));
        write(&corpus_path, corpus).await.unwrap_or_else(|e| {
            exits!(
                exitcode::IOERR,
                "Fail to persist corpus to {} : {}",
                corpus_path,
                e
            )
        });
        self.record.psersist().await;
    }

    async fn failed_analyze(&self, p: Prog, reason: Reason) {
        self.record.insert_failed(p, reason).await
    }

    async fn crash_analyze(&self, p: Prog, crash: Crash, executor: &mut Executor) {
        warn!("Trying to repo crash:{}", crash);
        let stmts = to_script(&p, &self.target);
        warn!("Caused by:\n{}", stmts.to_string());

        executor.start().await;
        match executor.exec(&p).await {
            Ok(exec_result) => {
                match exec_result {
                    ExecResult::Ok(_) => warn!("Repo failed, executed successfully"),
                    ExecResult::Failed(reason) => warn!("Repo failto, executed failed: {}", reason),
                };
                self.record.insert_crash(p, crash, false).await
            }
            Err(repo_crash) => {
                self.record
                    .insert_crash(p, repo_crash.unwrap_or(crash), true)
                    .await;
                warn!("Repo successfully, restarting guest ...");
                executor.start().await;
            }
        }
    }

    async fn feedback_analyze(
        &self,
        p: Prog,
        raw_blocks: Vec<Vec<usize>>,
        executor: &mut Executor,
    ) {
        for (call_index, raw_blocks) in raw_blocks.iter().enumerate() {
            let (new_blocks_1, new_branches_1) = self.feedback_info_of(raw_blocks).await;

            if !new_blocks_1.is_empty() || !new_branches_1.is_empty() {
                let p = p.sub_prog(call_index);
                let exec_result = self.exec_no_crash(executor, &p).await;

                if let ExecResult::Ok(raw_blocks) = exec_result {
                    if raw_blocks.len() == call_index + 1 {
                        let (new_block_2, new_branches_2) =
                            self.feedback_info_of(&raw_blocks[call_index]).await;

                        let new_block: HashSet<_> =
                            new_blocks_1.intersection(&new_block_2).cloned().collect();
                        let new_branches: HashSet<_> = new_branches_1
                            .intersection(&new_branches_2)
                            .cloned()
                            .collect();

                        if !new_block.is_empty() || !new_branches.is_empty() {
                            let minimized_p = minimize(&p, |_| true);
                            let raw_branches = self.exec_no_fail(executor, &minimized_p).await;

                            let mut blocks = Vec::new();
                            let mut branches = Vec::new();
                            for raw_branches in raw_branches.iter() {
                                let (block, branch) = self.cook_raw_block(raw_branches);
                                blocks.push(block);
                                branches.push(branch);
                            }

                            blocks.shrink_to_fit();
                            branches.shrink_to_fit();

                            self.record
                                .insert_executed(
                                    &minimized_p,
                                    &blocks[..],
                                    &branches[..],
                                    &new_block,
                                    &new_branches,
                                )
                                .await;
                            self.corpus.insert(minimized_p).await;
                            self.feedback.merge(new_block, new_branches).await;
                        }
                    }
                }
            }
        }
    }

    async fn feedback_info_of(&self, raw_blocks: &[usize]) -> (HashSet<Block>, HashSet<Branch>) {
        let (blocks, branches) = self.cook_raw_block(raw_blocks);
        let new_blocks = self.feedback.diff_block(&blocks[..]).await;
        let new_branches = self.feedback.diff_branch(&branches[..]).await;
        (new_blocks, new_branches)
    }

    /// calculate branch, return depuped blocks and branches
    fn cook_raw_block(&self, raw_blocks: &[usize]) -> (Vec<Block>, Vec<Branch>) {
        let mut blocks: Vec<Block> = raw_blocks.iter().map(|b| Block::from(*b)).collect();
        let mut branches: Vec<Branch> = blocks
            .iter()
            .cloned()
            .tuple_windows()
            .map(|(b1, b2)| Branch::from((b1, b2)))
            .collect();

        blocks.sort();
        blocks.dedup();
        blocks.shrink_to_fit();
        branches.sort();
        branches.dedup();
        branches.shrink_to_fit();
        (blocks, branches)
    }

    async fn exec_no_crash(&self, executor: &mut Executor, p: &Prog) -> ExecResult {
        match executor.exec(p).await {
            Ok(exec_result) => exec_result,
            Err(crash) => exits!(
                exitcode::SOFTWARE,
                "Unexpected crash: {}",
                crash.unwrap_or_default()
            ),
        }
    }

    async fn exec_no_fail(&self, executor: &mut Executor, p: &Prog) -> Vec<Vec<usize>> {
        match executor.exec(p).await {
            Ok(exec_result) => match exec_result {
                ExecResult::Ok(raw_branches) => raw_branches,
                ExecResult::Failed(reason) => {
                    exits!(exitcode::SOFTWARE, "Unexpected failed: {}", reason)
                }
            },
            Err(crash) => exits!(
                exitcode::SOFTWARE,
                "Unexpected crash: {}",
                crash.unwrap_or_default()
            ),
        }
    }

    async fn get_prog(&self) -> Prog {
        if let Some(p) = self.candidates.pop().await {
            p
        } else {
            gen(&self.target, &self.rt, &self.conf)
        }
    }
}
