use crate::{
    exec::{serialize, CallExecInfo, CrashInfo, ExecError, ExecHandle, ExecOpt, ExecResult},
    fuzz::{
        input::Input,
        mutation::{mutate_args, seq_reuse},
        queue::Queue,
    },
    gen::gen,
    model::{Prog, SyscallRef, TypeRef, Value},
    targets::Target,
};

use std::{
    cmp::max,
    collections::VecDeque,
    env::temp_dir,
    fs::write,
    path::PathBuf,
    process::Command,
    sync::RwLock,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rand::{thread_rng, Rng};
use rustc_hash::{FxHashMap, FxHashSet};

pub struct Crash;

/// Interesting values extracted inputs. not implemented yet.
pub type ValuePool = FxHashMap<TypeRef, VecDeque<Arc<Value>>>;

pub struct Fuzzer {
    // shared between different fuzzers.
    // TODO sync prog between diffierent threads.
    // progs: Arc<RwLock<FxHashMap<usize, Vec<ProgWrapper>>>>,
    max_cov: Arc<RwLock<FxHashSet<u32>>>,
    calibrated_cov: Arc<RwLock<FxHashSet<u32>>>,
    relations: Arc<RwLock<FxHashSet<(SyscallRef, SyscallRef)>>>,
    crashes: Arc<Mutex<Vec<Crash>>>,

    // local data.
    id: u32,
    target: Target,
    local_vals: ValuePool,
    queue: Queue,
    exec_handle: ExecHandle,
    run_history: VecDeque<Prog>,

    mode: Mode,
    mut_gaining: u32,
    gen_gaining: u32,
    cycle_len: u32,
    max_cycle_len: u32,

    work_dir: PathBuf,
    kernel_obj: Option<PathBuf>,
    kernel_src: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Sampling,
    Explore,
    Mutation,
}

impl Fuzzer {
    pub fn fuzz(&mut self) {
        self.sampling();

        loop {
            if self.mode == Mode::Explore {
                self.gen();
            } else {
                self.mutate();
            }
            self.update_mode();
        }
    }

    fn sampling(&mut self) {
        let mut i = 0;
        while self.queue.current_age < 1 && i < 128 {
            self.gen();
            self.mutate();
            i += 1;
        }

        let g0 = max(self.mut_gaining, 1);
        let g1 = max(self.gen_gaining, 1);
        let r0 = g0 as f64 / self.cycle_len as f64;
        let r1 = g1 as f64 / self.cycle_len as f64;

        if self.queue.current_age == 0 {
            log::warn!(
                "Fuzzer-{}: no culling occurred during the sampling, current fuzzing efficiency is too low (mutaion/gen: {}/{}).",
                self.id,
                r0,
                r1
            )
        } else {
            log::info!(
                "Fuzzer-{}: sampling done, culling: {}, mutaion/gen: {}/{})",
                self.id,
                self.queue.current_age,
                r0,
                r1
            );
        }
        self.update_mode();
    }

    fn update_mode(&mut self) {
        if self.queue.is_empty() || self.queue.current_age == 0 {
            self.mode = Mode::Explore;
            return;
        }

        let mut rng = thread_rng();
        let g0 = max(self.mut_gaining, 1);
        let g1 = max(self.gen_gaining, 1);
        if rng.gen_ratio(g0, g0 + g1) {
            self.mode = Mode::Mutation;
        } else {
            self.mode = Mode::Explore;
        }

        let r0 = g0 as f64 / self.cycle_len as f64;
        let r1 = g1 as f64 / self.cycle_len as f64;
        if self.cycle_len < self.max_cycle_len && (r0 < 0.005 || r1 < 0.005) {
            log::info!(
                "Fuzzer-{}: gaining too low(mut/gen {}/{}), scaling circle length({} -> {}).",
                self.id,
                r0,
                r1,
                self.cycle_len,
                self.cycle_len * 2
            );
            self.cycle_len *= 2;
        }
    }

    fn gen(&mut self) {
        self.gen_gaining = 0;
        for _ in 0..self.cycle_len {
            let p = gen(&self.target, &self.local_vals);
            if self.evaluate(p) {
                self.gen_gaining += 1;
            }
        }
    }

    fn mutate(&mut self) {
        use crate::fuzz::queue::AVG_SCORE;

        self.mut_gaining = 0;
        if self.queue.is_empty() {
            return;
        }

        let avg_score = self.queue.avgs[&AVG_SCORE];
        let mut mut_n = 0;

        while mut_n < self.cycle_len {
            let idx = self.queue.select_idx(true);
            let n = if self.queue.inputs[idx].score > avg_score {
                4
            } else {
                2
            };

            for _ in 0..n {
                let p = mutate_args(&self.queue.inputs[idx].p);
                if self.evaluate(p) {
                    self.mut_gaining += 1;
                    self.queue.inputs[idx].update_gaining_rate(true);
                } else {
                    self.queue.inputs[idx].update_gaining_rate(false);
                }
            }

            mut_n += n;

            for _ in 0..n {
                let p = seq_reuse(&self.queue.inputs[idx].p);
                if self.evaluate(p) {
                    self.mut_gaining += 1;
                    self.queue.inputs[idx].update_gaining_rate(true);
                } else {
                    self.queue.inputs[idx].update_gaining_rate(false);
                }
            }

            mut_n += n;
            // TODO add more mutation methods.
        }
    }

    fn evaluate(&mut self, p: Prog) -> bool {
        let r = self.exec_handle.exec(&ExecOpt::new(), &p);
        let exec_ret = match r {
            Ok(ret) => {
                if self.run_history.len() >= 64 {
                    self.run_history.pop_front();
                }
                self.run_history.push_back(p.clone());
                ret
            }
            Err(e) => {
                self.handle_err(e);
                return false;
            }
        };
        match exec_ret {
            ExecResult::Normal(info) => self.handle_info(p, info),
            ExecResult::Failed { info, err } => self.handle_failed(p, info, err, true),
            ExecResult::Crash(crash) => {
                self.handle_crash(p, crash);
                true
            }
        }
    }

    fn handle_err(&self, e: ExecError) {
        match e {
            ExecError::SyzInternal(e) => {
                if e.downcast_ref::<serialize::SerializeError>().is_none() {
                    log::warn!("Fuzzer-{}: failed to execute: {}", self.id, e);
                }
            }
            ExecError::Spawn(e) => {
                log::warn!("Fuzzer-{}: failed to execute: {}", self.id, e);
                // TODO
            }
        }
    }

    fn handle_failed(
        &mut self,
        mut p: Prog,
        mut info: Vec<CallExecInfo>,
        e: Box<dyn std::error::Error + 'static>,
        hanlde_info: bool,
    ) -> bool {
        log::info!("Fuzzer-{}: prog failed: {}.", self.id, e);

        if !hanlde_info {
            return false;
        }

        for i in (0..info.len()).rev() {
            if info[i].branches.is_empty() {
                info.remove(i);
                p.calls.drain(i..p.calls.len());
            } else {
                break;
            }
        }
        self.handle_info(p, info)
    }

    fn handle_info(&mut self, p: Prog, info: Vec<CallExecInfo>) -> bool {
        let (has_new, mut new_brs) = self.check_brs(&info, &self.max_cov);
        if !has_new {
            return false;
        }

        let (succ, _) = self.calibrate_cov(&p, &mut new_brs);
        if !succ {
            return false;
        }

        let mut analyzed: FxHashSet<usize> = FxHashSet::default();
        // analyze in reverse order helps us find interesting longger prog.
        for i in (0..info.len()).rev() {
            if info[i].branches.is_empty() || analyzed.contains(&i) {
                continue;
            }

            let mini_ret = self.minimize(p.sub_prog(i), &info[0..i + 1], &new_brs[i]);
            if mini_ret.is_none() {
                continue;
            }
            let (m_p, calls_idx, m_p_info) = mini_ret.unwrap(); // minimized prog.
            analyzed.extend(&calls_idx);
            let mut m_p_brs = calls_idx
                .into_iter()
                .map(|idx| new_brs[idx].iter().copied().collect())
                .collect::<Vec<_>>();

            let (succ, exec_tm) = self.calibrate_cov(&m_p, &mut m_p_brs);
            if !succ {
                continue;
            }

            let new_re = self.detect_relations(&m_p, &m_p_brs);

            let mut input = Input::new(m_p, ExecOpt::new_no_collide(), m_p_info);
            input.found_new_re = new_re;
            input.update_distinct_degree(&self.queue.call_cnt);
            input.age = self.queue.current_age;
            input.exec_tm = exec_tm.as_millis() as usize;
            input.new_cov = m_p_brs.into_iter().flatten().collect();
            input.update_score(&self.queue.avgs);

            self.queue.append(input);
        }
        true
    }

    fn minimize(
        &mut self,
        mut p: Prog,
        old_call_infos: &[CallExecInfo],
        new_brs: &FxHashSet<u32>,
    ) -> Option<(Prog, Vec<usize>, Vec<CallExecInfo>)> {
        if p.calls.len() <= 1 {
            return None;
        }

        let mut call_infos = None;
        let opt = ExecOpt::new();
        let old_len = p.calls.len();
        let mut removed = Vec::new();
        let mut idx = p.calls.len() - 1;

        for i in (0..p.calls.len() - 1).rev() {
            let new_p = p.remove(i);
            idx -= 1;
            let ret = self.exec_handle.exec(&opt, &new_p);
            if let Some(info) = self.handle_ret_comm(&new_p, ret) {
                let brs = info[idx].branches.iter().copied().collect::<FxHashSet<_>>();
                if brs.is_superset(new_brs) {
                    p = new_p;
                    removed.push(i);
                    call_infos = Some(info);
                } else {
                    idx += 1;
                }
            } else {
                return None;
            }
        }

        let reserved = (0..old_len).filter(|i| !removed.contains(i)).collect();
        Some((
            p,
            reserved,
            call_infos.unwrap_or_else(|| Vec::from(old_call_infos)),
        ))
    }

    fn detect_relations(&mut self, p: &Prog, brs: &[FxHashSet<u32>]) -> bool {
        if p.calls.len() == 1 {
            return false;
        }

        let mut detected = false;
        let opt = ExecOpt::new_no_collide();
        for i in 0..p.calls.len() - 1 {
            let new_p = p.remove(i);
            let ret = self.exec_handle.exec(&opt, &new_p);
            if let Some(info) = self.handle_ret_comm(&new_p, ret) {
                if !brs[i].iter().all(|br| info[i].branches.contains(br)) {
                    let s0 = p.calls[i].meta;
                    let s1 = new_p.calls[i].meta;
                    if self.add_relation((s0, s1)) {
                        detected = true
                    }
                }
            }
        }

        detected
    }

    fn add_relation(&mut self, (s0, s1): (SyscallRef, SyscallRef)) -> bool {
        {
            let r = self.relations.read().unwrap();
            if r.contains(&(s0, s1)) {
                return false;
            }
        }

        log::info!(
            "Fuzzer-{}: detect new relation: ({}, {}).",
            self.id,
            s0.name,
            s1.name
        );
        let mut r = self.relations.write().unwrap();
        r.insert((s0, s1))
    }

    fn check_brs(
        &self,
        info: &[CallExecInfo],
        covs: &RwLock<FxHashSet<u32>>,
    ) -> (bool, Vec<FxHashSet<u32>>) {
        let mut has_new = false;
        let mut new_brs = Vec::with_capacity(info.len());

        {
            let covs = covs.read().unwrap();
            for i in info.iter() {
                let mut new_br = FxHashSet::default();
                for br in i.branches.iter().copied() {
                    if !covs.contains(&br) {
                        new_br.insert(br);
                    }
                }
                if !new_br.is_empty() {
                    has_new = true;
                }
                new_brs.push(new_br);
            }
        }
        if has_new {
            let mut covs = covs.write().unwrap();
            for br in new_brs.iter() {
                covs.extend(br.iter().copied());
            }
        }
        (has_new, new_brs)
    }

    fn calibrate_cov(&mut self, p: &Prog, new_covs: &mut [FxHashSet<u32>]) -> (bool, Duration) {
        let opt = ExecOpt::new_no_collide();
        let mut failed = false;
        let mut exec_tm = Duration::new(0, 0);

        for _ in 0..3 {
            let now = Instant::now();
            let ret = self.exec_handle.exec(&opt, p);
            exec_tm += now.elapsed();
            if let Some(info) = self.handle_ret_comm(p, ret) {
                let (_, covs) = self.check_brs(&info, &self.calibrated_cov);
                for i in 0..new_covs.len() {
                    new_covs[i] = new_covs[i].intersection(&covs[i]).cloned().collect();
                }
            } else {
                failed = true;
                break;
            }
        }

        (
            !failed && new_covs.iter().any(|c| !c.is_empty()),
            exec_tm / 3,
        )
    }

    fn handle_ret_comm(
        &mut self,
        p: &Prog,
        ret: Result<ExecResult, ExecError>,
    ) -> Option<Vec<CallExecInfo>> {
        match ret {
            Ok(exec_ret) => match exec_ret {
                ExecResult::Normal(info) => Some(info),
                ExecResult::Failed { info, err } => {
                    self.handle_failed(p.clone(), info, err, false);
                    None
                }
                ExecResult::Crash(c) => {
                    self.handle_crash(p.clone(), c);
                    None
                }
            },
            Err(e) => {
                self.handle_err(e);
                None
            }
        }
    }

    fn handle_crash(&mut self, p: Prog, info: CrashInfo) {
        let log = &info.qemu_stdout;
        let _log_str = String::from_utf8_lossy(log);
        let tmp_file = temp_dir().join(format!("healer-crash-log-{}.tmp", self.id));

        if let Err(e) = write(&tmp_file, &log) {
            log::error!(
                "Fuzzer-{}: failed to write crash log to tmp file '{}': {}",
                self.id,
                tmp_file.display(),
                e
            );
            self.save_raw_log(p, info.qemu_stdout);
            return;
        }
        let bin_path = self.work_dir.join("bin").join("syz-syz-symbolize");
        let mut syz_symbolize = Command::new(&bin_path);
        syz_symbolize
            .args(vec!["-os", &self.target.os])
            .args(vec!["-arch", &self.target.arch]);

        if let Some(kernel_obj) = self.kernel_obj.as_ref() {
            syz_symbolize.arg("-kernel_obj").arg(kernel_obj);
        }
        if let Some(kernel_src) = self.kernel_obj.as_ref() {
            syz_symbolize.arg("-kernel_src").arg(kernel_src);
        }
        let _out = syz_symbolize.output().unwrap();

        todo!()
    }

    fn save_raw_log(&mut self, _p: Prog, _log: Vec<u8>) {
        todo!()
    }
}
