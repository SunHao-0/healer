use crate::{
    exec::{ExecHandle, ExecOpt},
    fuzz::queue::Queue,
    gen::gen,
    model::{Prog, TypeRef, Value},
    targets::Target,
};

use std::sync::{Arc, Mutex};
use std::{collections::VecDeque, sync::RwLock};

use rustc_hash::{FxHashMap, FxHashSet};

pub struct Crash;

pub type ValuePool = FxHashMap<TypeRef, VecDeque<Arc<Value>>>;

#[allow(dead_code)] // todo
pub struct Fuzzer {
    // shared between different fuzzers.
    progs: Arc<RwLock<FxHashMap<usize, Vec<Arc<Prog>>>>>,
    vals: Arc<RwLock<FxHashMap<usize, ValuePool>>>,
    max_cov: Arc<Mutex<FxHashSet<u32>>>,
    calibrated_cov: Arc<Mutex<FxHashSet<u32>>>,
    crashes: Arc<Mutex<Vec<Crash>>>,

    // local data.
    target: Target,
    local_vals: ValuePool,
    queue: Queue,
    exec_handle: ExecHandle,
    opt: ExecOpt,
    run_history: VecDeque<Arc<Prog>>,
}

impl Fuzzer {
    pub fn fuzz(&mut self) {
        loop {
            if self.should_explore() {
                let _p = gen(&self.target, &self.local_vals);
            } else {
                self.mutate();
            }
        }
    }

    fn should_explore(&self) -> bool {
        todo!()
    }

    fn mutate(&mut self) {
        todo!()
    }

    // fn exec(&mut self, p: Prog) {
    //     match self.exec_handle.exec(&self.opt, &p) {
    //         Ok(exec_ret) => match exec_ret {
    //             ExecResult::Normal(info) => self.save_if_interesting(p, info),
    //             ExecResult::Failed { info, err } => {
    //                 log::info!("prog failed: {}\n{}", err, &p);
    //                 self.save_if_interesting(p, info);
    //             }
    //             ExecResult::Crash(info) => {
    //                 self.save_crash(p, info);
    //             }
    //         },
    //         Err(e) => {
    //             log::warn!("{}\n{}", e, p);
    //         }
    //     }
    // }

    // fn save_if_interesting(&mut self, p: Prog, infos: Vec<CallExecInfo>) {
    //     todo!()
    // }

    // fn calibrate(&self) {
    //     todo!()
    // }

    // fn save_crash(&mut self, p: Prog, info: CrashInfo) {}
}
