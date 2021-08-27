use crate::util::stop_soon;
use std::thread::sleep;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[derive(Debug, Default)]
pub struct Stats {
    fuzzing: AtomicU64,
    repro: AtomicU64,
    relations: AtomicU64,
    crashes: AtomicU64,
    unique_crash: AtomicU64,
    crash_suppressed: AtomicU64,
    vm_restarts: AtomicU64,
    corpus_size: AtomicU64,
    exec_total: AtomicU64,
    corpus_cov: AtomicU64,
    max_cov: AtomicU64,
}

impl Stats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_fuzzing(&self) {
        self.fuzzing.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_fuzzing(&self) {
        self.fuzzing.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_repro(&self) {
        self.repro.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_repro(&self) {
        self.repro.fetch_sub(1, Ordering::Relaxed);
    }

    // pub max_cov: AtomicU64,
    pub fn inc_re(&self) {
        self.relations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_unique_crash(&self) {
        self.unique_crash.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_crashes(&self) {
        self.crashes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_crash_suppressed(&self) {
        self.crash_suppressed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_vm_restarts(&self) {
        self.vm_restarts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_corpus_size(&self) {
        self.corpus_size.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_exec_total(&self) {
        self.exec_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_corpus_cov(&self, n: u64) {
        self.corpus_cov.fetch_add(n, Ordering::Relaxed);
    }

    pub fn add_max_cov(&self, n: u64) {
        self.max_cov.fetch_add(n, Ordering::Relaxed);
    }

    pub fn report(&self, duration: Duration) {
        while !stop_soon() {
            sleep(duration);

            let fuzzing = self.fuzzing.load(Ordering::Relaxed);
            let repro = self.repro.load(Ordering::Relaxed);
            let relations = self.relations.load(Ordering::Relaxed);
            let crashes = self.crashes.load(Ordering::Relaxed);
            let unique_crash = self.unique_crash.load(Ordering::Relaxed);
            let corpus_size = self.corpus_size.load(Ordering::Relaxed);
            let exec_total = self.exec_total.load(Ordering::Relaxed);
            let corpus_cov = self.corpus_cov.load(Ordering::Relaxed);
            let max_cov = self.max_cov.load(Ordering::Relaxed);
            log::info!("exec: {}, fuzz/repro {}/{}, unique/crash {}/{}, cov/max {}/{}, relations: {}, corpus: {}",
            exec_total, fuzzing, repro, unique_crash, crashes, corpus_cov, max_cov, relations, corpus_size);
        }
    }
}
