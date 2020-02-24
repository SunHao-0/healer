use crate::feedback::{Block, Branch};
use crate::guest::Crash;
use chrono::prelude::*;
use chrono::DateTime;
use core::c::{translate, Script};
use core::prog::Prog;
use core::target::Target;
use executor::Reason;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::fs::write;
use tokio::sync::Mutex;

pub struct TestCaseRecord {
    normal: Mutex<Vec<ExecutedCase>>,
    failed: Mutex<Vec<FailedCase>>,
    target: Arc<Target>,
    id_n: Mutex<usize>,

    normal_num: Mutex<usize>,
    failed_num: Mutex<usize>,
    crashed_num: Mutex<usize>,
}

#[derive(Serialize)]
struct TestCase {
    id: usize,
    title: String,
    test_time: DateTime<Local>,
}

#[derive(Serialize)]
struct ExecutedCase {
    meta: TestCase,
    /// execute test program
    p: String,
    /// number of blocks per call
    block_num: Vec<usize>,
    /// number of branchs per call
    branch_num: Vec<usize>,
    /// new branch of last call
    new_branch: usize,
    /// new block of last call
    new_block: usize,
}

#[derive(Serialize)]
struct FailedCase {
    meta: TestCase,
    p: String,
    reason: String,
}

#[derive(Serialize)]
struct CrashedCase {
    meta: TestCase,
    p: String,
    crash: Crash,
}

impl TestCaseRecord {
    pub fn new(t: Arc<Target>) -> Self {
        Self {
            normal: Mutex::new(Vec::new()),
            failed: Mutex::new(Vec::new()),
            target: t,
            id_n: Mutex::new(0),
            normal_num: Mutex::new(0),
            failed_num: Mutex::new(0),
            crashed_num: Mutex::new(0),
        }
    }

    pub async fn insert_executed(
        &self,
        p: &Prog,
        blocks: &[Vec<Block>],
        branches: &[Vec<Branch>],
        new_block: &HashSet<Block>,
        new_branch: &HashSet<Branch>,
    ) {
        let block_num = blocks.iter().map(|blocks| blocks.len()).collect();
        let branch_num = branches.iter().map(|branches| branches.len()).collect();
        let stmts = translate(&p, &self.target);
        let title = self.title_of(&p, &stmts);

        let case = ExecutedCase {
            meta: TestCase {
                id: self.next_id().await,
                title,
                test_time: Local::now(),
            },
            p: stmts.to_string(),
            block_num,
            branch_num,
            new_branch: new_branch.len(),
            new_block: new_block.len(),
        };
        {
            let mut execs = self.normal.lock().await;
            execs.push(case);
        }
        {
            let mut exec_n = self.normal_num.lock().await;
            *exec_n += 1;
        }
        self.try_persist_normal_case().await;
    }

    pub async fn insert_crash(&self, p: Prog, crash: Crash) {
        let stmts = translate(&p, &self.target);
        let case = CrashedCase {
            meta: TestCase {
                id: self.next_id().await,
                title: self.title_of(&p, &stmts),
                test_time: Local::now(),
            },
            p: stmts.to_string(),
            crash,
        };
        {
            let mut crashed_num = self.crashed_num.lock().await;
            *crashed_num += 1;
        }
        self.persist_crash_case(case).await
    }

    pub async fn insert_failed(&self, p: Prog, reason: Reason) {
        let stmts = translate(&p, &self.target);
        let case = FailedCase {
            meta: TestCase {
                id: self.next_id().await,
                title: self.title_of(&p, &stmts),
                test_time: Local::now(),
            },
            p: stmts.to_string(),
            reason: reason.to_string(),
        };
        {
            let mut failed_cases = self.failed.lock().await;
            failed_cases.push(case);
        }
        {
            let mut failed_num = self.failed_num.lock().await;
            *failed_num += 1;
        }
        self.try_persist_failed_case().await
    }

    pub async fn psersist(&self) {
        tokio::join!(
            self.try_persist_normal_case(),
            self.try_persist_failed_case()
        );
    }

    async fn try_persist_normal_case(&self) {
        const MAX_NORMAL_NUM: usize = 256;
        let mut cases = Vec::new();
        {
            let mut normal_cases = self.normal.lock().await;
            if normal_cases.len() < MAX_NORMAL_NUM {
                return;
            }
            std::mem::swap(&mut cases, &mut normal_cases);
        }
        let path = format!("./{}/{}_{}", "reports", "n", Local::now());
        let report = serde_json::to_string_pretty(&cases).unwrap();
        write(&path, report).await.unwrap_or_else(|e| {
            exits!(
                exitcode::IOERR,
                "Fail to persist normal test case to {} : {}",
                path,
                e
            )
        })
    }

    async fn try_persist_failed_case(&self) {
        const MAX_FAILED_NUM: usize = 32;
        let mut cases = Vec::new();
        {
            let mut failed_cases = self.failed.lock().await;
            if failed_cases.len() < MAX_FAILED_NUM {
                return;
            }
            std::mem::swap(&mut cases, &mut failed_cases);
        }
        let path = format!("./{}/{}_{}", "reports", "f", Local::now());
        let report = serde_json::to_string_pretty(&cases).unwrap();
        write(&path, report).await.unwrap_or_else(|e| {
            exits!(
                exitcode::IOERR,
                "Fail to persist failed test case to {} : {}",
                path,
                e
            )
        })
    }

    async fn persist_crash_case(&self, case: CrashedCase) {
        let path = format!("./{}/{}", "crashes", case.meta.title);
        let crash = serde_json::to_string_pretty(&case).unwrap();
        write(&path, crash).await.unwrap_or_else(|e| {
            exits!(
                exitcode::IOERR,
                "Fail to persist failed test case to {} : {}",
                path,
                e
            )
        })
    }

    fn title_of(&self, p: &Prog, stmts: &Script) -> String {
        let group = String::from(self.target.group_name_of(p.gid));
        let target_call = stmts.0.last().unwrap().to_string();
        format!("{}: {}", group, target_call)
    }

    async fn next_id(&self) -> usize {
        let mut id = self.id_n.lock().await;
        let next = *id;
        *id += 1;
        next
    }
}
