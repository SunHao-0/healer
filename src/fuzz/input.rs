use crate::{
    exec::{CallExecInfo, ExecOpt},
    model::{Prog, SyscallRef},
};

use std::sync::Arc;

use rustc_hash::FxHashMap;

/// An interesting prog with related fuzz data.
///
/// An input contains a prog that covers new branches. The related performance data,
/// such as depth, size, execution time, are used to evaluate the quality of current
/// prog in multiple aspects.  
pub struct Input {
    /// Prog that may find new branches.
    pub(crate) p: Arc<Prog>,
    /// Execution option of prog.
    pub(crate) opt: ExecOpt,
    /// Execution result of prog, with execution option `opt`.
    pub(crate) info: Vec<CallExecInfo>,
    /// Had mutation since add to queue?  
    pub(crate) was_mutated: bool,
    /// Prog contains new branches.
    pub(crate) favored: bool,
    /// Found any new relation?
    pub(crate) found_new_re: bool,
    /// All syscalls can be executed successfully in a clean state OS.
    pub(crate) self_contained: bool,
    /// Overall score of prog.
    pub(crate) score: usize,
    /// The rate of gaining of mutating the current input.
    pub(crate) gaining_rate: usize,
    /// The difference degree between current prog and other prog.
    pub(crate) distinct_degree: usize,
    /// Number of queue culling since appended.
    pub(crate) age: usize,
    /// Depth of mutation.
    pub(crate) depth: usize,
    /// Size of the whole prog.
    pub(crate) sz: usize,
    /// Length of the prog.
    pub(crate) len: usize,
    /// Execution time, in ms.
    pub(crate) exec_tm: usize,
    /// Number of contained resources.
    pub(crate) res_cnt: usize,
    /// New coverage this prog found.
    pub(crate) new_cov: Vec<u32>,

    mutation_cnt: usize,
    gain_cnt: usize,
    last_update: usize,
}

impl Input {
    pub fn update_gaining_rate(&mut self, gain: bool) -> usize {
        self.mutation_cnt += 1;
        self.last_update += 1;
        if gain {
            self.gain_cnt += 1;
        }
        if self.last_update >= 32 {
            self.gaining_rate =
                (((self.gain_cnt as f64) / (self.mutation_cnt as f64)) * 100.0) as usize;
            self.last_update = 0;
        }
        self.gaining_rate
    }

    pub fn update_distinct_degree(&mut self, cnt: &FxHashMap<SyscallRef, usize>) -> usize {
        let avg_cnt = ((cnt.values().sum::<usize>() as f64) / (cnt.len() as f64)).ceil() as usize;
        let mut current_cnt = Vec::new();
        for call in self.p.calls.iter() {
            current_cnt.push(cnt[&call.meta]);
        }
        let mut degree = 0;
        for cnt in current_cnt {
            if avg_cnt > cnt {
                degree += avg_cnt - cnt;
            }
        }
        self.distinct_degree = degree;
        degree
    }

    pub fn update_score(&mut self, avg: &FxHashMap<usize, usize>) -> usize {
        // Use a static score sheet.
        // was_mutated      => 30,
        // favored          => 50,
        // found_new_re     => 50,
        // self_contained   => 50,
        // distinct_degree  => 30,
        // gaining_rate     => 10,
        // age              => 10,
        // depth            => 10,
        // sz               => 10,
        // exec_tm          => 10,
        // res_cnt          => 10,
        // new_cov          => 10,
        // TODO make this more adaptive.
        use crate::fuzz::queue::*;
        let mut score = 0;
        if self.favored && !self.was_mutated {
            score += 50;
        } else if self.favored && self.was_mutated {
            score += 30;
        } else if !self.was_mutated {
            score += 10;
        }
        if self.found_new_re {
            score += 50;
        }
        if self.self_contained {
            score += 50;
        }
        let avg_degree = avg[&AVG_DISTINCT_DEGREE];
        if self.distinct_degree > avg_degree * 2 {
            let delta = (self.distinct_degree - avg_degree * 2) as f64 / (avg_degree as f64) * 10.0;
            let delta = std::cmp::min(10, delta.ceil() as usize);
            score += 20 + delta;
        } else if self.distinct_degree > avg_degree {
            let delta = (self.distinct_degree - avg_degree) as f64 / (avg_degree as f64) * 10.0;
            score += 10 + delta.ceil() as usize;
        } else {
            let delta = self.distinct_degree as f64 / (avg_degree as f64);
            score += (10.0 * delta) as usize;
        }
        score += self.gaining_rate / 10;
        if self.len > avg[&AVG_LEN] {
            score += 30;
        }
        if self.age < avg[&AVG_AGE] {
            score += 10;
        }
        if self.depth < avg[&AVG_DEPTH] {
            score += 10;
        }
        if self.sz < avg[&AVG_SZ] {
            score += 10;
        }
        if self.exec_tm < avg[&AVG_EXEC_TM] {
            score += 10;
        };
        if self.res_cnt < avg[&AVG_RES_CNT] {
            score += 10;
        }
        if self.new_cov.len() < avg[&AVG_NEW_COV] {
            score += 10;
        }
        self.score = score;
        score
    }

    pub fn desciption(&self) -> String {
        todo!()
    }
}
