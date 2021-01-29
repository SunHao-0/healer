use crate::{
    exec::{serialize, CallExecInfo, CrashInfo, ExecError, ExecHandle, ExecOpt, ExecResult},
    fuzz::{
        input::Input,
        mutation::{mutate_args, seq_reuse},
        queue::Queue,
    },
    gen::gen,
    model::{Prog, TypeRef, Value},
    targets::Target,
};

use std::{
    cmp::max,
    collections::VecDeque,
    sync::RwLock,
    sync::{Arc, Mutex},
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
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
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
        for _ in 0..128 {
            self.gen();
            self.mutate();
        }
        log::info!(
            "Fuzzer-{}: sampling done. (mutaion/gen, {}/{})",
            self.id,
            self.mut_gaining,
            self.gen_gaining
        );
        self.update_mode();
    }

    fn update_mode(&mut self) {
        if self.queue.is_empty() {
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
            if self.exec(p) {
                self.gen_gaining += 1;
            }
        }
    }

    fn mutate(&mut self) {
        use crate::fuzz::queue::AVG_SCORE;

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
                if self.exec(p) {
                    self.mut_gaining += 1;
                    self.queue.inputs[idx].update_gaining_rate(true);
                } else {
                    self.queue.inputs[idx].update_gaining_rate(false);
                }
            }

            mut_n += n;

            for _ in 0..n {
                let p = seq_reuse(&self.queue.inputs[idx].p);
                if self.exec(p) {
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

    fn exec(&mut self, p: Prog) -> bool {
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
            ExecResult::Failed { info, err } => self.handle_failed(p, info, err),
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
    ) -> bool {
        log::info!("Fuzzer-{}: prog failed: {}.", self.id, e);

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
        let (has_new, mut new_brs) = self.check_brs(&info);
        if !has_new {
            return false;
        }
        if !self.calibrate_cov(&p, &mut new_brs) {
            return false;
        }
        let mut analyzed: FxHashSet<usize> = FxHashSet::default();
        // analyze in reverse order helps us find interesting longger prog.
        for i in (0..info.len()).rev() {
            if info[i].branches.is_empty() || analyzed.contains(&i) {
                continue;
            }
            let (m_p, calls_idx, m_p_info) = self.minimize(p.sub_prog(i)); // minimized prog.
            analyzed.extend(&calls_idx);

            let m_p_brs = calls_idx
                .into_iter()
                .map(|idx| new_brs[idx].iter().copied().collect())
                .collect::<Vec<_>>();
            let new_re = self.detect_relations(&m_p, &m_p_brs);
            //                depth: (),
            //                exec_tm: (),
            let mut input = Input::new(m_p, ExecOpt::new_no_collide(), m_p_info);
            input.found_new_re = new_re;
            input.update_distinct_degree(&self.queue.call_cnt);
            input.age = self.queue.current_age;
            input.new_cov = m_p_brs.into_iter().flatten().collect();
            input.update_score(&self.queue.avgs);
            self.queue.append(input);
        }
        true
    }

    fn minimize(&mut self, _p: Prog) -> (Prog, Vec<usize>, Vec<CallExecInfo>) {
        todo!()
    }

    fn detect_relations(&mut self, _p: &Prog, _brs: &[FxHashSet<u32>]) -> bool {
        todo!()
    }

    fn check_brs(&self, info: &[CallExecInfo]) -> (bool, Vec<FxHashSet<u32>>) {
        let mut has_new = false;
        let mut new_brs = Vec::with_capacity(info.len());

        {
            let max_cov = self.max_cov.read().unwrap();
            for i in info.iter() {
                let mut new_br = FxHashSet::default();
                for br in i.branches.iter().copied() {
                    if !max_cov.contains(&br) {
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
            let mut max_brs = self.max_cov.write().unwrap();
            for br in new_brs.iter() {
                max_brs.extend(br.iter().copied());
            }
        }
        (has_new, new_brs)
    }

    fn calibrate_cov(&mut self, _p: &Prog, _new_covs: &mut [FxHashSet<u32>]) -> bool {
        let _cov = self.calibrated_cov.read().unwrap();
        todo!()
    }

    fn handle_crash(&mut self, _p: Prog, _info: CrashInfo) {
        let _crashes = self.crashes.lock();
        todo!()
    }
}
