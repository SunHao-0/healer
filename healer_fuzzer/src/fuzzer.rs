use crate::{
    config::Config, crash::CrashManager, feedback::Feedback, fuzzer_log::set_fuzzer_id,
    prepare_exec_env, spawn_syz, stats::Stats, util::stop_soon,
};
use anyhow::Context;
use healer_core::{
    corpus::CorpusWrapper,
    gen::{gen_prog, minimize},
    mutation::mutate,
    prog::Prog,
    relation::RelationWrapper,
    target::Target,
    HashSet, RngType,
};
use healer_vm::qemu::QemuHandle;
use sha1::Digest;
use std::{
    cell::Cell,
    collections::VecDeque,
    fs::{create_dir_all, write},
    io::ErrorKind,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use syz_wrapper::{
    exec::{
        features::FEATURE_FAULT, ExecError, ExecOpt, ExecutorHandle, CALL_FAULT_INJECTED,
        FLAG_COLLIDE, FLAG_INJECT_FAULT,
    },
    report::extract_report,
    repro::repro,
};

pub struct SharedState {
    pub(crate) target: Arc<Target>,
    pub(crate) relation: Arc<RelationWrapper>,
    pub(crate) corpus: Arc<CorpusWrapper>,
    pub(crate) stats: Arc<Stats>,
    pub(crate) feedback: Arc<Feedback>,
    pub(crate) crash: Arc<CrashManager>,
}

impl Clone for SharedState {
    fn clone(&self) -> Self {
        Self {
            target: Arc::clone(&self.target),
            relation: Arc::clone(&self.relation),
            corpus: Arc::clone(&self.corpus),
            stats: Arc::clone(&self.stats),
            feedback: Arc::clone(&self.feedback),
            crash: Arc::clone(&self.crash),
        }
    }
}

pub struct Fuzzer {
    pub shared_state: SharedState,

    // local
    pub id: u64,
    pub rng: RngType,
    pub executor: ExecutorHandle,
    pub qemu: QemuHandle,
    pub last_reboot: Instant,
    pub run_history: VecDeque<(ExecOpt, Prog)>,
    pub config: Config,
}

pub const HISTORY_CAPACITY: usize = 1024;

impl Fuzzer {
    pub fn fuzz_loop(&mut self, progs: Vec<Prog>) -> anyhow::Result<()> {
        const GENERATE_PERIOD: u64 = 100;

        set_fuzzer_id(self.id);
        self.shared_state.stats.inc_fuzzing();
        log::info!("fuzzer-{} online", self.id);

        let mut err = None;

        for prog in progs {
            if let Err(e) = self
                .execute_one(prog)
                .context("failed to execute input prog")
            {
                err = Some(e);
            }
        }
        if let Some(e) = err {
            self.shared_state.stats.dec_fuzzing();
            return Err(e);
        }

        for i in 0_u64.. {
            // TODO update period based on gaining
            if self.shared_state.corpus.is_empty() || i % GENERATE_PERIOD == 0 {
                let p = gen_prog(
                    &self.shared_state.target,
                    &self.shared_state.relation,
                    &mut self.rng,
                );
                if let Err(e) = self
                    .execute_one(p)
                    .context("failed to execute generated prog")
                {
                    err = Some(e);
                    break;
                }
            } else {
                let mut p = self.shared_state.corpus.select_one(&mut self.rng).unwrap();
                mutate(
                    &self.shared_state.target,
                    &self.shared_state.relation,
                    &self.shared_state.corpus,
                    &mut self.rng,
                    &mut p,
                );
                if let Err(e) = self
                    .execute_one(p)
                    .context("failed to execute mutated prog")
                {
                    err = Some(e);
                    break;
                }
            }

            if stop_soon() {
                break;
            }
        }

        self.shared_state.stats.dec_fuzzing();
        if let Some(e) = err {
            Err(e)
        } else {
            Ok(())
        }
    }

    pub fn execute_one(&mut self, p: Prog) -> anyhow::Result<bool> {
        let opt = ExecOpt::new();
        self.record_execution(&p, &opt);
        let ret = self
            .executor
            .execute_one(&self.shared_state.target, &p, &opt);
        self.shared_state.stats.inc_exec_total();

        match ret {
            Ok(prog_info) => {
                let mut new_cov = false;
                let mut calls: Vec<(usize, HashSet<u32>)> = Vec::with_capacity(p.calls().len());

                for (idx, call_info) in prog_info.call_infos.into_iter().enumerate() {
                    let new = self
                        .shared_state
                        .feedback
                        .check_max_cov(call_info.branches.iter().copied());
                    if !new.is_empty() {
                        new_cov = true;
                        calls.push((idx, call_info.branches.iter().copied().collect()));
                    }
                }
                if let Some(extra) = prog_info.extra {
                    self.shared_state.feedback.check_max_cov(extra.branches);
                    // TODO handle extra
                }

                for (idx, brs) in calls {
                    self.save_if_new(&p, idx, brs)?;
                }

                self.clear_vm_log();
                self.maybe_reboot_vm()?;
                Ok(new_cov)
            }
            Err(e) => {
                if let Some(crash) = self.check_vm(&p, e) {
                    self.handle_crash(&p, crash)
                        .context("failed to handle crash")?;
                    Ok(true)
                } else {
                    self.restart_exec()?;
                    Ok(false)
                }
            }
        }
    }

    fn save_if_new(&mut self, p: &Prog, idx: usize, brs: HashSet<u32>) -> anyhow::Result<()> {
        let mut new = self
            .shared_state
            .feedback
            .check_cal_cov(brs.iter().copied());
        if new.is_empty() {
            return Ok(());
        }
        let syscall = self.shared_state.target.syscall_of(p.calls()[idx].sid());
        fuzzer_trace!("[{}] new cov: {}", syscall.name(), new.len());

        // calibrate new cov
        let mut failed = 0;
        for _ in 0..3 {
            let ret = self.reexec(p, idx)?;
            if ret.is_none() {
                failed += 1;
                if failed > 2 {
                    return Ok(());
                }
                continue;
            }
            let brs = ret.unwrap();
            new = new.intersection(&brs).copied().collect();
            if new.is_empty() {
                return Ok(());
            }
        }

        // minimize
        let mut p = p.clone();
        let target = Arc::clone(&self.shared_state.target); //TODO
        minimize(&target, &mut p, idx, |p, new_idx| {
            for _ in 0..3 {
                if let Ok(Some(brs)) = self.reexec(p, new_idx) {
                    return brs.intersection(&new).copied().count() == new.len();
                }
            }
            false
        });

        self.do_save_prog(p.clone(), &brs)?;
        if self.config.features.unwrap() & FEATURE_FAULT != 0 && self.config.enable_fault_injection
        {
            self.fail_call(&p, idx)?;
        }
        Ok(())
    }

    fn fail_call(&mut self, p: &Prog, idx: usize) -> anyhow::Result<()> {
        let t = Arc::clone(&self.shared_state.target);
        let mut opt = ExecOpt::new();
        opt.enable(FLAG_INJECT_FAULT);
        opt.fault_call = idx as i32;
        for i in 0..100 {
            opt.fault_nth = i;
            self.record_execution(p, &opt);
            let ret = self.executor.execute_one(&t, p, &opt);
            match ret {
                Ok(info) => {
                    if info.call_infos.len() > idx
                        && info.call_infos[idx].flags & CALL_FAULT_INJECTED == 0
                    {
                        break;
                    }
                }
                Err(e) => {
                    if let Some(crash) = self.check_vm(p, e) {
                        self.handle_crash(p, crash)
                            .context("failed to handle crash")?;
                    } else {
                        self.restart_exec()?;
                    }
                }
            }
        }
        Ok(())
    }

    fn do_save_prog(&mut self, p: Prog, cov: &HashSet<u32>) -> anyhow::Result<()> {
        let mut hasher = sha1::Sha1::new();
        let p_str = p.display(&self.shared_state.target).to_string();
        hasher.update(p_str.as_bytes());
        let sha1 = hasher.finalize();
        let out = self.config.output.join("corpus");
        if let Err(e) = create_dir_all(&out) {
            if e.kind() != ErrorKind::AlreadyExists {
                return Err(e).context("failed to  create corpus dir");
            }
        }
        write(out.join(&hex::encode(sha1)), p_str.as_bytes()).context("failed to write prog")?;

        self.shared_state.corpus.add_prog(p, cov.len() as u64);
        self.shared_state.stats.inc_corpus_size();
        self.shared_state.feedback.merge(cov);
        self.shared_state
            .stats
            .set_max_cov(self.shared_state.feedback.max_cov_len() as u64);
        self.shared_state
            .stats
            .set_cal_cov(self.shared_state.feedback.cal_cov_len() as u64);

        Ok(())
    }

    fn reexec(&mut self, p: &Prog, idx: usize) -> anyhow::Result<Option<HashSet<u32>>> {
        let mut opt = ExecOpt::new();
        opt.disable(FLAG_COLLIDE);
        let ret = self
            .executor
            .execute_one(&self.shared_state.target, p, &opt);
        self.shared_state.stats.inc_exec_total();

        match ret {
            Ok(info) => {
                if info.call_infos.len() > idx && !info.call_infos[idx].branches.is_empty() {
                    let brs = info.call_infos[idx].branches.iter().copied().collect();
                    Ok(Some(brs))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                if let Some(crash) = self.check_vm(p, e) {
                    self.handle_crash(p, crash)?;
                }
                Ok(None)
            }
        }
    }

    fn check_vm(&mut self, p: &Prog, e: ExecError) -> Option<Vec<u8>> {
        fuzzer_trace!("failed to exec prog: {}", e);

        let crash_error = !matches!(
            e,
            ExecError::ProgSerialization(_) | ExecError::OutputParse(_)
        );
        if crash_error && !self.qemu.is_alive() {
            fuzzer_warn!(
                "kernel crashed, maybe caused by:\n{}",
                p.display(&self.shared_state.target)
            );
            let log = self.qemu.collect_crash_log().unwrap();
            Some(log)
        } else {
            None
        }
    }

    fn handle_crash(&mut self, p: &Prog, crash_log: Vec<u8>) -> anyhow::Result<()> {
        if stop_soon() {
            return Ok(());
        }

        self.shared_state.stats.inc_crashes();
        let ret = extract_report(&self.config.report_config, p, &crash_log);
        match ret.as_deref() {
            Ok([report, ..]) => {
                let title = report.title.clone();
                fuzzer_info!("crash: {}", title);
                let need_repro = self
                    .shared_state
                    .crash
                    .save_new_report(&self.shared_state.target, report.clone())?;
                if need_repro {
                    fuzzer_info!("trying to repro...",);
                    self.try_repro(&title, &crash_log)
                        .context("failed to repro")?;
                }
            }
            _ => {
                if !crash_log.is_empty() {
                    fuzzer_info!("failed to extract report, saving to raw logs",);
                    self.shared_state.crash.save_raw_log(&crash_log)?;
                }
            }
        }

        self.reboot_vm()
    }

    fn try_repro(&mut self, title: &str, crash_log: &[u8]) -> anyhow::Result<()> {
        if stop_soon() {
            return Ok(());
        }

        self.shared_state.stats.inc_repro();
        self.shared_state.stats.dec_fuzzing();
        let history = self.run_history.make_contiguous();
        let repro = repro(
            &self.config.repro_config,
            &self.shared_state.target,
            crash_log,
            history,
        )
        .context("failed to repro")?;
        self.shared_state.stats.dec_repro();
        self.shared_state.stats.inc_fuzzing();
        self.shared_state.crash.repro_done(title, repro)?;
        self.shared_state
            .stats
            .set_unique_crash(self.shared_state.crash.unique_crashes());

        Ok(())
    }

    fn record_execution(&mut self, p: &Prog, opt: &ExecOpt) {
        if self.run_history.len() >= HISTORY_CAPACITY {
            self.run_history.pop_front();
        }
        self.run_history.push_back((opt.clone(), p.clone()))
    }

    fn clear_vm_log(&mut self) {
        thread_local! {
            static LAST_CLEAR: Cell<u64> = Cell::new(0)
        }
        let n = LAST_CLEAR.with(|v| {
            let n = v.get();
            v.set(n + 1);
            n + 1
        });
        if n >= 64 {
            self.qemu.reset();
        }
    }

    fn restart_exec(&mut self) -> anyhow::Result<()> {
        let syz_exec = PathBuf::from("~/syz-executor"); // TODO fix this
        spawn_syz(&syz_exec, &self.qemu, &mut self.executor)
            .with_context(|| format!("failed to spawn syz-executor for fuzzer-{}", self.id))
    }

    #[inline]
    fn reboot_vm(&mut self) -> anyhow::Result<()> {
        let ret = prepare_exec_env(&self.config, &mut self.qemu, &mut self.executor)
            .context("failed to reboot");
        self.last_reboot = Instant::now();
        ret
    }

    fn maybe_reboot_vm(&mut self) -> anyhow::Result<()> {
        let du = self.last_reboot.elapsed();
        if du >= Duration::from_secs(60 * 60) {
            // restart 1  every hour
            self.reboot_vm()?;
            self.shared_state.stats.inc_vm_restarts();
        }
        Ok(())
    }
}
