use crate::corpus::Corpus;
use crate::feedback::FeedBack;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;
use circular_queue::CircularQueue;
use core::prog::Prog;
use std::sync::Arc;
use tokio::fs::write;
use tokio::sync::broadcast;
use tokio::time;
use tokio::time::Duration;

pub struct StatSource {
    pub corpus: Arc<Corpus>,
    pub feedback: Arc<FeedBack>,
    pub candidates: Arc<CQueue<Prog>>,
    pub record: Arc<TestCaseRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub corpus: usize,
    pub blocks: usize,
    pub branches: usize,
    // pub exec:usize,
    // pub gen:usize,
    // pub minimized:usize,
    pub candidates: usize,
    pub normal_case: usize,
    pub failed_case: usize,
    pub crashed_case: usize,
}

pub struct Sampler {
    pub source: StatSource,
    pub interval: Duration,
    pub stats: CircularQueue<Stats>,
    pub shutdown: broadcast::Receiver<()>,
    pub work_dir: String,
}

impl Sampler {
    pub async fn sample(&mut self) {
        use broadcast::TryRecvError::*;
        loop {
            match self.shutdown.try_recv() {
                Ok(_) => {
                    self.persist().await;
                    return;
                }
                Err(e) => match e {
                    Empty => (),
                    Closed | Lagged(_) => panic!("Unexpected braodcast receiver state"),
                },
            }

            time::delay_for(self.interval).await;
            let (corpus, (blocks, branches), candidates, (normal_case, failed_case, crashed_case)) = tokio::join!(
                self.source.corpus.len(),
                self.source.feedback.len(),
                self.source.candidates.len(),
                self.source.record.len()
            );
            let stat = Stats {
                corpus,
                blocks,
                branches,
                candidates,
                normal_case,
                failed_case,
                crashed_case,
            };
            self.stats.push(stat);
            info!("corpus {},blocks {},branches {},candidates {},normal_case {},failed_case {},crashed_case {}", corpus, blocks, branches, candidates, normal_case, failed_case, crashed_case);
        }
    }

    async fn persist(&self) {
        if self.stats.is_empty() {
            return;
        }

        let stats = self.stats.asc_iter().cloned().collect::<Vec<_>>();
        let path = format!("{}/stats.json", self.work_dir);
        let stats = serde_json::to_string_pretty(&stats).unwrap();
        write(&path, stats).await.unwrap_or_else(|e| {
            exits!(exitcode::IOERR, "Fail to persist stats to {} : {}", path, e)
        })
    }
}
