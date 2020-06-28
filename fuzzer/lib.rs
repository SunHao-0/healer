use std::path::PathBuf;
use std::process::{exit, id};
use std::sync::Arc;

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate log;

use regex::Regex;
use tokio::fs::{create_dir_all, read};
use tokio::signal::ctrl_c;
use tokio::sync::{broadcast, Barrier};
use tokio::time::{delay_for, Duration, Instant};

use core::prog::Prog;
use core::target::Target;
use fots::types::Items;

use crate::exec::{Executor, ExecutorConf};
use crate::fuzzer::Fuzzer;
use crate::guest::{GuestConf, QemuConf, SSHConf};
#[cfg(feature = "mail")]
use crate::mail::MailConf;
use crate::stats::SamplerConf;

#[macro_use]
#[allow(dead_code)]
mod utils;
pub mod corpus;
mod exec;
pub mod feedback;
mod fuzzer;
mod guest;
#[cfg(feature = "mail")]
mod mail;
pub mod report;
mod stats;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub fots_bin: PathBuf,
    pub curpus: Option<PathBuf>,
    pub vm_num: usize,
    pub suppressions: Option<Vec<String>>,
    pub ignores: Option<Vec<String>>,
    pub guest: GuestConf,
    pub qemu: QemuConf,
    pub ssh: SSHConf,
    pub executor: ExecutorConf,
    pub sampler: Option<SamplerConf>,

    #[cfg(feature = "mail")]
    pub mail: Option<MailConf>,
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

        if let Some(suppressions) = &self.suppressions {
            for s in suppressions {
                Regex::new(&s).unwrap_or_else(|e| {
                    eprintln!(
                        "Config Error: suppressions regex \"{}\" compile failed: {}",
                        s, e
                    );
                    exit(exitcode::CONFIG)
                });
            }
        }

        if let Some(ignores) = &self.ignores {
            for i in ignores {
                Regex::new(&i).unwrap_or_else(|e| {
                    eprintln!(
                        "Config Error: ignores regex \"{}\" compile failed: {}",
                        i, e
                    );
                    exit(exitcode::CONFIG)
                });
            }
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

        if let Some(sampler) = self.sampler.as_ref() {
            sampler.check()
        }

        #[cfg(feature = "mail")]
        if let Some(mail) = mail.as_ref() {
            mail.check()
        }

        self.guest.check();
        self.executor.check();
        self.qemu.check();
        self.ssh.check();
    }
}

pub async fn fuzz(cfg: Config) {
    let cfg = Arc::new(cfg);
    let (target, corpus) = tokio::join!(load_target(&cfg), load_corpus(&cfg.curpus));
    check_corpus(&target, &corpus);
    info!("Corpus: {}", corpus.len());
    info!(
        "Syscalls: {}  Groups: {}",
        target.fns.len(),
        target.groups.len()
    );

    let fuzzer = Fuzzer::new(target, corpus, &cfg);
    info!(
        "Booting {} {}/{} on {} ...",
        cfg.vm_num, cfg.guest.os, cfg.guest.arch, cfg.guest.platform
    );
    let now = std::time::Instant::now();
    let shutdown = start_fuzz(fuzzer.clone(), cfg.clone()).await;
    info!("Boot finished, cost {}s.", now.elapsed().as_secs());

    wait_exit_signal(fuzzer, shutdown).await
}

async fn start_fuzz(fuzzer: Fuzzer, cfg: Arc<Config>) -> broadcast::Sender<()> {
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let barrier = Arc::new(Barrier::new(cfg.vm_num + 1));
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

    let stats_source = fuzzer.stats();
    tokio::spawn(async move {
        let mut sampler = stats::Sampler::new(stats_source);
        sampler.sample(&cfg.sampler, shutdown_rx).await;
    });
    shutdown_tx
}

async fn wait_exit_signal(fuzzer: Fuzzer, shutdown: broadcast::Sender<()>) {
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
    shutdown.send(()).unwrap();
    fuzzer.persist().await;

    let now = Instant::now();
    let wait_time = Duration::new(5, 0);
    while shutdown.receiver_count() != 0 {
        delay_for(Duration::from_millis(200)).await;
        if now.elapsed() >= wait_time {
            warn!("Wait time out, force to exit...");
            exit(exitcode::SOFTWARE);
        }
    }
    info!("All done");
    // TODO clear resources when exiting, e.g. qemu process.
    exit(exitcode::OK);
}

fn check_corpus(t: &Target, corpus: &[Prog]) {
    for p in corpus.iter() {
        if !t.groups.contains_key(&p.gid) {
            eprintln!("Config Error: fots_bin/corpus not match: corpus contains unknown groups");
            exit(1);
        }
        for c in p.calls.iter() {
            if !t.fns.contains_key(&c.fid) {
                eprintln!("Config Error: fots_bin/corpus not match: corpus contains unknown fn");
                exit(1);
            }
        }
    }
}

async fn load_corpus(path: &Option<PathBuf>) -> Vec<Prog> {
    if let Some(path) = path.as_ref() {
        let data = read(path).await.unwrap();
        bincode::deserialize(&data).unwrap()
    } else {
        Vec::new()
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
    let pid = id(); // pid
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
