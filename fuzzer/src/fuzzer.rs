use crate::corpus::Corpus;
use crate::exec::ExecHandle;
use crate::feedback::FeedBack;
use crate::utils::process::Handle;
use crate::utils::queue::CQueue;
use core::analyze::RTable;
use core::gen::gen;
use core::prog::Prog;
use core::target::Target;
use executor::ExecResult;
use fots::types::GroupId;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Fuzzer {
    pub target: Target,
    pub rt: HashMap<GroupId, RTable>,
    pub conf: core::gen::Config,
    pub corpus: Arc<Corpus>,
    pub feedback: Arc<FeedBack>,
    pub candidates: Arc<CQueue<Prog>>,
}

impl Fuzzer {
    pub async fn fuzz(&self, _qemu: Handle, mut executor: ExecHandle) {
        loop {
            let p = self.get_prog().await;
            let exec_result = executor.exec(&p);

            if self.has_new_branches(exec_result).await {
                let p = self.minimize(&p);
                self.corpus.insert(p).await;
            }
        }
    }

    async fn get_prog(&self) -> Prog {
        if let Some(p) = self.candidates.pop().await {
            p
        } else {
            gen(&self.target, &self.rt, &self.conf)
        }
    }

    fn minimize(&self, p: &Prog) -> Prog {
        p.clone()
    }

    async fn has_new_branches(&self, exec_result: ExecResult) -> bool {
        let mut has = false;
        for branches in exec_result.0.into_iter() {
            if self.feedback.merge(branches).await.is_some() {
                has = true;
            }
        }
        has
    }
}
