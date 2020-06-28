use crate::corpus::Corpus;
use crate::feedback::FeedBack;
#[cfg(feature = "mail")]
use crate::mail;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;
#[cfg(feature = "mail")]
use lettre_email::EmailBuilder;

use circular_queue::CircularQueue;
use core::prog::Prog;
use std::process::exit;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    pub exec: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub corpus: usize,
    pub blocks: usize,
    pub branches: usize,
    pub exec: usize,
    // pub gen:usize,
    // pub minimized:usize,
    pub candidates: usize,
    pub normal_case: usize,
    pub failed_case: usize,
    pub crashed_case: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SamplerConf {
    /// Duration for sampling, per second
    pub sample_interval: u64,
    /// Duration for report, per minites
    pub report_interval: u64,
}

impl Default for SamplerConf {
    fn default() -> Self {
        Self {
            sample_interval: 15,
            report_interval: 60,
        }
    }
}

impl SamplerConf {
    pub fn check(&self) {
        if self.sample_interval < 10
            || self.report_interval <= 10
            || self.sample_interval * 60 < self.report_interval
        {
            eprintln!("Config Error: invalid sample conf: sample interval should longger than 10s, \
                                    report internval should long than 10m and sample interval should \
                                    not longger than report interval");
            exit(exitcode::CONFIG)
        }
    }
}

pub struct Sampler {
    pub source: StatSource,
    pub stats: CircularQueue<Stats>,
}

impl Sampler {
    pub fn new(source: StatSource) -> Self {
        Self {
            source,
            stats: CircularQueue::with_capacity(1024),
        }
    }
    pub async fn sample(
        &mut self,
        conf: &Option<SamplerConf>,
        mut shutdown: broadcast::Receiver<()>,
    ) {
        let interval = match conf {
            Some(SamplerConf {
                sample_interval,
                report_interval,
            }) => (
                Duration::new(*sample_interval, 0),
                Duration::new(report_interval * 60, 0),
            ),
            None => (Duration::new(15, 0), Duration::new(60 * 60, 0)),
        };
        tokio::select! {
            _ = shutdown.recv() => (),
            _ = self.do_sample(interval) => (),
        }
        self.persist().await;
    }

    async fn do_sample(&mut self, (sample_interval, report_interval): (Duration, Duration)) {
        let mut last_report = Duration::new(0, 0);
        loop {
            time::delay_for(sample_interval).await;
            last_report += sample_interval;

            let (corpus, (blocks, branches), candidates, (normal_case, failed_case, crashed_case)) = tokio::join!(
                self.source.corpus.len(),
                self.source.feedback.len(),
                self.source.candidates.len(),
                self.source.record.len()
            );
            let exec = self.source.exec.load(Ordering::SeqCst);

            let stat = Stats {
                exec,
                corpus,
                blocks,
                branches,
                candidates,
                normal_case,
                failed_case,
                crashed_case,
            };

            if report_interval <= last_report {
                #[cfg(feature = "mail")]
                self.report(&stat).await;
                last_report = Duration::new(0, 0);
            }

            self.stats.push(stat);
            info!(
                "exec {}, blocks {}, branches {}, failed {}, crashed {}",
                exec, blocks, branches, failed_case, crashed_case
            );
        }
    }

    async fn persist(&self) {
        if self.stats.is_empty() {
            return;
        }

        let stats = self.stats.asc_iter().cloned().collect::<Vec<_>>();
        let path = "./stats.json";
        let stats = serde_json::to_string_pretty(&stats).unwrap();
        write(&path, stats).await.unwrap_or_else(|e| {
            exits!(exitcode::IOERR, "Fail to persist stats to {} : {}", path, e)
        })
    }

    #[cfg(feature = "mail")]
    async fn report(&self, stat: &Stats) {
        let stat = serde_json::to_string_pretty(&stat).unwrap();
        let email = EmailBuilder::new()
            .subject("Healer-Stats Regular Report")
            .body(stat);
        mail::send(email).await
    }
}
