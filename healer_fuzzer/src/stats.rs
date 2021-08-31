use crate::util::stop_soon;
use std::thread::sleep;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[derive(Debug, Default)]
pub(crate) struct Stats {
    fuzzing: AtomicU64,
    repro: AtomicU64,
    relations: AtomicU64,
    crashes: AtomicU64,
    unique_crash: AtomicU64,
    // crash_suppressed: AtomicU64,
    vm_restarts: AtomicU64,
    corpus_size: AtomicU64,
    exec_total: AtomicU64,
    cal_cov: AtomicU64,
    max_cov: AtomicU64,
}

impl Stats {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn inc_fuzzing(&self) {
        self.fuzzing.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn dec_fuzzing(&self) {
        self.fuzzing.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn inc_repro(&self) {
        self.repro.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn dec_repro(&self) {
        self.repro.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn set_re(&self, n: u64) {
        self.relations.store(n, Ordering::Relaxed);
    }

    pub(crate) fn set_unique_crash(&self, n: u64) {
        self.unique_crash.store(n, Ordering::Relaxed);
    }

    pub(crate) fn inc_crashes(&self) {
        self.crashes.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn inc_vm_restarts(&self) {
        self.vm_restarts.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn inc_corpus_size(&self) {
        self.corpus_size.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn inc_exec_total(&self) {
        self.exec_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn set_cal_cov(&self, n: u64) {
        self.cal_cov.store(n, Ordering::Relaxed);
    }

    pub(crate) fn set_max_cov(&self, n: u64) {
        self.max_cov.store(n, Ordering::Relaxed);
    }

    pub(crate) fn report(&self, duration: Duration) {
        while !stop_soon() {
            sleep(duration);

            let fuzzing = self.fuzzing.load(Ordering::Relaxed);
            let repro = self.repro.load(Ordering::Relaxed);
            let relations = self.relations.load(Ordering::Relaxed);
            let crashes = self.crashes.load(Ordering::Relaxed);
            let unique_crash = self.unique_crash.load(Ordering::Relaxed);
            let corpus_size = self.corpus_size.load(Ordering::Relaxed);
            let exec_total = self.exec_total.load(Ordering::Relaxed);
            let corpus_cov = self.cal_cov.load(Ordering::Relaxed);
            let max_cov = self.max_cov.load(Ordering::Relaxed);
            log::info!(
                "exec: {}, fuzz/repro {}/{}, uniq/total crashes {}/{}, cal/max cover {}/{}, re: {}, corpus: {}",
                exec_total,
                fuzzing,
                repro,
                unique_crash,
                crashes,
                corpus_cov,
                max_cov,
                relations,
                corpus_size
            );
        }
    }
}
