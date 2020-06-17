use crate::corpus::Corpus;
use crate::exec::Executor;
use crate::feedback::{Block, Branch, FeedBack};
use crate::guest::Crash;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;
use core::analyze::prog_analyze;
use core::analyze::RTable;
use core::c::to_prog;
use core::gen::gen;
use core::minimize::remove;
use core::mutate::mutate;
use core::prog::Prog;
use core::target::Target;
use executor::{ExecResult, Reason};
use fots::types::GroupId;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs::write;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Fuzzer {
    pub target: Arc<Target>,
    pub rt: Arc<Mutex<HashMap<GroupId, RTable>>>,
    pub conf: core::gen::Config,
    pub corpus: Arc<Corpus>,
    pub feedback: Arc<FeedBack>,
    pub candidates: Arc<CQueue<Prog>>,
    pub record: Arc<TestCaseRecord>,
    pub exec_cnt: Arc<AtomicUsize>,
    pub crash_digests: Arc<Mutex<HashSet<md5::Digest>>>,
}

impl Fuzzer {
    pub async fn fuzz(self, executor: Executor, mut shutdown: broadcast::Receiver<()>) {
        tokio::select! {
            _ = shutdown.recv() => (),
            _ = self.do_fuzz(executor) => ()
        }
    }

    async fn do_fuzz(&self, mut executor: Executor) {
        let mut gen_cnt = 0;
        loop {
            let p = self.get_prog(&mut gen_cnt).await;
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
            };
            self.exec_cnt.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub async fn persist(self) {
        let corpus_path = "./corpus";
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
        warn!("========== Crashed ========= \n{}", crash);
        let p_str = to_prog(&p, &self.target);
        warn!("Caused by:\n{}", p_str);

        if self.need_repo(&crash.inner).await {
            warn!("Restarting to repro ...");
            executor.start().await;

            self.exec_cnt.fetch_add(1, Ordering::SeqCst);
            match executor.exec(&p).await {
                Ok(exec_result) => {
                    match exec_result {
                        ExecResult::Ok(_) => warn!("Repo failed, executed successfully"),
                        ExecResult::Failed(reason) => {
                            warn!("Repo failed, executed failed: {}", reason)
                        }
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
        } else {
            info!("Restarting, ignoring useless crash ...");
            executor.start().await;
        }
    }

    async fn need_repo(&self, crash: &str) -> bool {
        if crash.contains("CRASH-MEMLEAK") || crash == "$$" || crash.is_empty() {
            return false;
        }
        let digest = md5::compute(crash);
        let mut g = self.crash_digests.lock().await;
        g.insert(digest)
    }

    async fn feedback_analyze(
        &self,
        p: Prog,
        raw_blocks: Vec<Vec<usize>>,
        executor: &mut Executor,
    ) {
        for (call_index, raw_blocks) in raw_blocks.iter().enumerate() {
            let (new_blocks_1, new_branches_1) = self.check_new_feedback(raw_blocks).await;

            if !new_blocks_1.is_empty() || !new_branches_1.is_empty() {
                let p = p.sub_prog(call_index);
                let exec_result = self.exec_no_crash(executor, &p).await;

                if let ExecResult::Ok(raw_blocks) = exec_result {
                    if raw_blocks.len() == call_index + 1 {
                        let (new_block_2, new_branches_2) =
                            self.check_new_feedback(&raw_blocks[call_index]).await;

                        let new_block: HashSet<_> =
                            new_blocks_1.intersection(&new_block_2).cloned().collect();
                        let new_branches: HashSet<_> = new_branches_1
                            .intersection(&new_branches_2)
                            .cloned()
                            .collect();

                        if !new_block.is_empty() || !new_branches.is_empty() {
                            let minimized_p = self.minimize(&p, &new_block, executor).await;
                            let raw_branches = self.exec_no_fail(executor, &minimized_p).await;
                            {
                                let g = &self.target.groups[&p.gid];
                                let mut r = self.rt.lock().await;
                                prog_analyze(g, r.get_mut(&p.gid).unwrap(), &p);
                            }

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

    async fn minimize(
        &self,
        p: &Prog,
        new_block: &HashSet<Block>,
        executor: &mut Executor,
    ) -> Prog {
        assert!(!p.calls.is_empty());

        let mut p = p.clone();
        if p.len() == 1 {
            return p;
        }

        let mut p_orig;
        let mut i = 0;
        while i != p.len() - 1 {
            p_orig = p.clone();
            if !remove(&mut p, i) {
                i += 1;
            } else if let ExecResult::Ok(cover) = self.exec_no_crash(executor, &p).await {
                let (new_blocks_1, _) = self.check_new_feedback(cover.last().unwrap()).await;
                if new_blocks_1.is_empty() || new_blocks_1.intersection(new_block).count() == 0 {
                    i += 1;
                    p = p_orig;
                }
            } else {
                p = p_orig;
                return p;
            }
        }
        p
    }

    async fn check_new_feedback(&self, raw_blocks: &[usize]) -> (HashSet<Block>, HashSet<Branch>) {
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
        self.exec_cnt.fetch_add(1, Ordering::SeqCst);
        match executor.exec(p).await {
            Ok(exec_result) => exec_result,
            Err(crash) => {
                self.crash_analyze(p.clone(), crash.unwrap_or_default(), executor)
                    .await;
                ExecResult::Failed(Reason(String::from("Crashed")))
            }
        }
    }

    async fn exec_no_fail(&self, executor: &mut Executor, p: &Prog) -> Vec<Vec<usize>> {
        self.exec_cnt.fetch_add(1, Ordering::SeqCst);
        match executor.exec(p).await {
            Ok(exec_result) => match exec_result {
                ExecResult::Ok(raw_branches) => raw_branches,
                ExecResult::Failed(_) => Default::default(),
            },
            Err(crash) => {
                self.crash_analyze(p.clone(), crash.unwrap_or_default(), executor)
                    .await;
                Default::default()
            }
        }
    }

    async fn get_prog(&self, gen_cnt: &mut usize) -> Prog {
        if let Some(p) = self.candidates.pop().await {
            p
        } else if self.corpus.is_empty().await || *gen_cnt % 100 != 0 {
            *gen_cnt += 1;
            let rt = self.rt.lock().await;
            gen(&self.target, &rt, &self.conf)
        } else {
            let rt = {
                let rt = self.rt.lock().await;
                rt.clone()
            };
            let corpus = self.corpus.inner.lock().await;
            mutate(&corpus, &self.target, &rt, &self.conf)
        }
    }
}
