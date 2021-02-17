use crate::{
    exec::{CallExecInfo, ExecOpt},
    model::Prog,
};

use std::fmt::Write;

/// An interesting prog with related fuzz data.
///
/// An input contains a prog that covers new branches. The related performance data,
/// such as depth, size, execution time, are used to evaluate the quality of current
/// prog in multiple aspects.  
pub struct Input {
    /// Prog that may find new branches.
    pub(crate) p: Prog,
    /// Execution option of prog.
    pub opt: ExecOpt,
    /// Execution result of prog, with execution option `opt`.
    pub(crate) info: Vec<CallExecInfo>,
    /// Had mutation since add to queue?  
    pub(crate) was_mutated: bool,
    /// Prog contains new branches.
    pub(crate) favored: bool,
    /// Found any new relation?
    pub(crate) found_new_re: bool,
    /// Overall score of prog.
    pub(crate) score: usize,
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
    /// Fault injection count.
    pub(crate) fault_injected: bool,
}

impl Input {
    pub fn new(p: Prog, opt: ExecOpt, info: Vec<CallExecInfo>, new_cov: Vec<u32>) -> Self {
        let len = p.calls.len();
        let sz = p.calls.iter().map(|c| c.val_cnt).sum();
        let res_cnt = p.calls.iter().map(|c| c.res_cnt).sum();
        let depth = p.depth;
        let mut inp = Self {
            p,
            opt,
            info,
            was_mutated: false,
            favored: !new_cov.is_empty(),
            found_new_re: false,
            score: 0,
            age: 1,
            depth,
            sz,
            len,
            exec_tm: 0,
            res_cnt,
            new_cov,
            fault_injected: false,
        };
        inp.update_score();
        inp
    }

    pub fn update_score(&mut self) -> usize {
        let age = if self.age > 1 {
            (self.age as f64) * 0.6
        } else {
            1.0
        };
        let score = if !self.new_cov.is_empty() {
            self.new_cov.len() as f64 / age
        } else {
            1.0
        };
        self.score = std::cmp::max(1, score as usize);
        self.score
    }

    pub fn desciption(&self) -> String {
        let mut name = format!(
            "len:{},score:{},dep:{},age:{},",
            self.p.calls.len(),
            self.score,
            self.depth,
            self.age
        );
        if !self.was_mutated {
            name.push_str("new,")
        }
        if self.favored {
            name.push_str("fav,")
        }
        if self.found_new_re {
            name.push_str("re,");
        }
        if !self.new_cov.is_empty() {
            write!(name, "+cov:{},", self.new_cov.len()).unwrap();
        }

        name.pop();
        name
    }
}
