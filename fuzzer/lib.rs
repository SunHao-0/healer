#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

use crate::corpus::Corpus;
use crate::exec::{Executor, ExecutorConf};
use crate::feedback::FeedBack;
use crate::fuzzer::Fuzzer;
use crate::guest::{GuestConf, QemuConf, SSHConf};
#[cfg(feature = "mail")]
use crate::mail::MailConf;
use crate::report::TestCaseRecord;
use crate::utils::queue::CQueue;

use crate::stats::SamplerConf;
use circular_queue::CircularQueue;
use core::analyze::static_analyze;
use core::prog::Prog;
use core::target::Target;
use fots::types::Items;
use stats::StatSource;
use std::path::PathBuf;
use std::process;
use std::process::exit;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::fs::{create_dir_all, read};
use tokio::signal::ctrl_c;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, Barrier};
use tokio::time::{delay_for, Duration, Instant};

#[macro_use]
pub mod utils;
pub mod corpus;
pub mod exec;
pub mod feedback;
pub mod fuzzer;
pub mod guest;
#[cfg(feature = "mail")]
pub mod mail;
pub mod report;
pub mod stats;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fots_bin: PathBuf,
    pub curpus: Option<PathBuf>,
    pub vm_num: usize,

    pub guest: GuestConf,
    pub qemu: Option<QemuConf>,
    pub ssh: Option<SSHConf>,

    pub executor: ExecutorConf,
    #[cfg(feature = "mail")]
    pub mail: Option<MailConf>,
    pub sampler: Option<SamplerConf>,
}

impl Config {
    pub fn check(&self) {
        if !self.fots_bin.is_file() {
            eprintln!(
                "Config Error: fots file {} is invalid",
                self.fots_bin.display()
            );
            exit(exitcode::CONFIG)
        }

        if let Some(corpus) = &self.curpus {
            if !corpus.is_file() {
                eprintln!("Config Error: corpus file {} is invalid", corpus.display());
                exit(exitcode::CONFIG)
            }
        }

        let cpu_num = num_cpus::get();
        if self.vm_num == 0 || self.vm_num > cpu_num * 8 {
            eprintln!(
                "Config Error: invalid vm num {}, vm num must between (0,{}] on your system",
                self.vm_num,
                cpu_num * 8
            );
            exit(exitcode::CONFIG)
        }

        self.guest.check();
        self.executor.check();

        if let Some(qemu) = self.qemu.as_ref() {
            qemu.check()
        }

        if let Some(ssh) = self.ssh.as_ref() {
            ssh.check();
        }

        #[cfg(feature = "mail")]
        mail_check(&self.mail);

        if let Some(sampler) = self.sampler.as_ref() {
            sampler.check()
        }
    }
}

#[cfg(feature = "mail")]
fn mail_check(mail: &Option<MailConf>) {
    if let Some(mail) = mail.as_ref() {
        mail.check()
    }
}

pub async fn fuzz(cfg: Config) {
    let cfg = Arc::new(cfg);
    let (target, candidates) = tokio::join!(load_target(&cfg), load_candidates(&cfg.curpus));
    info!("Corpus: {}", candidates.len().await);
    info!(
        "Syscalls: {}  Groups: {}",
        target.fns.len(),
        target.groups.len()
    );

    // init system state, shared between multi tasks
    let target = Arc::new(target);
    let candidates = Arc::new(candidates);
    let corpus = Arc::new(Corpus::default());
    let feedback = Arc::new(FeedBack::default());
    let record = Arc::new(TestCaseRecord::new(target.clone()));
    let rt = static_analyze(&target);
    let exec_cnt = Arc::new(AtomicUsize::new(0));
    let fuzzer = Fuzzer {
        target,
        exec_cnt: exec_cnt.clone(),
        rt: Arc::new(Mutex::new(rt)),
        conf: Default::default(),
        candidates: candidates.clone(),
        corpus: corpus.clone(),
        feedback: feedback.clone(),
        record: record.clone(),
    };
    let stats_source = StatSource {
        exec: exec_cnt,
        corpus,
        feedback,
        candidates,
        record,
    };

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let barrier = Arc::new(Barrier::new(cfg.vm_num + 1));
    info!(
        "Booting {} {}/{} on {} ...",
        cfg.vm_num, cfg.guest.os, cfg.guest.arch, cfg.guest.platform
    );
    let now = std::time::Instant::now();
    for _ in 0..cfg.vm_num {
        let cfg = cfg.clone();
        let fuzzer = fuzzer.clone();
        let barrier = barrier.clone();
        let shutdown = shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut executor = Executor::new(&cfg);
            executor.start().await;
            barrier.wait().await;
            fuzzer.fuzz(executor, shutdown).await;
        });
    }

    barrier.wait().await;
    info!("Boot finished, cost {}s.", now.elapsed().as_secs());

    tokio::spawn(async move {
        let mut sampler = stats::Sampler {
            source: stats_source,
            stats: CircularQueue::with_capacity(1024),
        };
        sampler.sample(&cfg.sampler, shutdown_rx).await;
    });

    if cfg!(unix) {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sig_ir =
            signal(SignalKind::interrupt()).expect("failed to set up SIGINT signal handler");
        let mut sig_term =
            signal(SignalKind::terminate()).expect("failed to set up SIGTERM signal handler");
        info!("Send SIGINT or SIGTERM to stop fuzzer");
        tokio::select! {
            _ = sig_ir.recv() => {
                  warn!("INTERUPTE signal recved");
            }
            _= sig_term.recv() => {
                    warn!("TERM signal signal recved");
            }
        }
    } else {
        info!("Send SIGINT to stop fuzzer");
        ctrl_c()
            .await
            .expect("failed to set up ctrl-c signal handler");
        warn!("INTERUPTE signal recved");
    }

    warn!("Stopping, persisting data...");
    shutdown_tx.send(()).unwrap();
    fuzzer.persist().await;

    let now = Instant::now();
    let wait_time = Duration::new(5, 0);
    while shutdown_tx.receiver_count() != 0 {
        delay_for(Duration::from_millis(200)).await;
        if now.elapsed() >= wait_time {
            warn!("Wait time out, force to exit...");
            exit(exitcode::SOFTWARE);
        }
    }
    info!("All done");
    exit(exitcode::OK);
}

async fn load_candidates(path: &Option<PathBuf>) -> CQueue<Prog> {
    if let Some(path) = path.as_ref() {
        let data = read(path).await.unwrap();
        let progs: Vec<Prog> = bincode::deserialize(&data).unwrap();
        CQueue::from(progs)
    } else {
        CQueue::default()
    }
}

async fn load_target(cfg: &Config) -> Target {
    let items = Items::load(&read(&cfg.fots_bin).await.unwrap_or_else(|e| {
        error!("Fail to load fots file: {}", e);
        exit(exitcode::DATAERR);
    }))
    .unwrap();
    Target::from(items)
}

pub async fn prepare_env() {
    init_logger();
    let pid = process::id();
    std::env::set_var("HEALER_FUZZER_PID", format!("{}", pid));
    info!("Pid: {}", pid);

    use tokio::io::ErrorKind::*;
    if let Err(e) = create_dir_all("./crashes").await {
        if e.kind() != AlreadyExists {
            exits!(exitcode::IOERR, "Fail to create crash dir: {}", e);
        }
    }
}

fn init_logger() {
    use log::LevelFilter;
    use log4rs::append::console::ConsoleAppender;
    use log4rs::append::file::FileAppender;
    use log4rs::append::rolling_file::policy::compound::{roll, trigger, CompoundPolicy};
    use log4rs::append::rolling_file::RollingFileAppender;
    use log4rs::config::{Appender, Config, Logger, Root};
    use log4rs::encode::pattern::PatternEncoder;

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S)} {h({l})} {t} - {m}{n}",
        )))
        .build();

    let fuzzer_appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S)} {h({l})} - {m}{n}",
        )))
        .build("log/fuzzer.log")
        .unwrap();

    let stats_trigger = trigger::size::SizeTrigger::new(1024 * 1024 * 100);
    let stats_roll = roll::fixed_window::FixedWindowRoller::builder()
        .build("stats.log.{}", 2)
        .unwrap();
    let stats_policy = CompoundPolicy::new(Box::new(stats_trigger), Box::new(stats_roll));
    let stats_appender = RollingFileAppender::builder()
        .append(false)
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S)} {h({l})} - {m}{n}",
        )))
        .build("log/stats.log", Box::new(stats_policy))
        .unwrap();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("fuzzer_appender", Box::new(fuzzer_appender)))
        .appender(Appender::builder().build("stats_appender", Box::new(stats_appender)))
        .logger(
            Logger::builder()
                .appender("stats_appender")
                .build("fuzzer::stats", LevelFilter::Info),
        )
        .logger(
            Logger::builder()
                .appender("fuzzer_appender")
                .build("fuzzer::fuzzer", LevelFilter::Info),
        )
        .build(Root::builder().appender("stdout").build(LevelFilter::Info))
        .unwrap();
    log4rs::init_config(config).unwrap();
}

const HEALER: &str = r"
 ___   ___   ______   ________   __       ______   ______
/__/\ /__/\ /_____/\ /_______/\ /_/\     /_____/\ /_____/\
\::\ \\  \ \\::::_\/_\::: _  \ \\:\ \    \::::_\/_\:::_ \ \
 \::\/_\ .\ \\:\/___/\\::(_)  \ \\:\ \    \:\/___/\\:(_) ) )_
  \:: ___::\ \\::___\/_\:: __  \ \\:\ \____\::___\/_\: __ `\ \
   \: \ \\::\ \\:\____/\\:.\ \  \ \\:\/___/\\:\____/\\ \ `\ \ \
    \__\/ \::\/ \_____\/ \__\/\__\/ \_____\/ \_____\/ \_\/ \_\/
";

pub fn show_info() {
    println!("{}", HEALER);
}
