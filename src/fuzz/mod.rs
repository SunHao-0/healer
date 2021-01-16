use crate::model::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub struct GlobalState {
    branches: (),
    covers: (),
    corpus: (),
}

pub type ValuePool = FxHashMap<Rc<Type>, FxHashSet<Value>>;

pub struct LocalState {
    pub branches: (),
    pub cover: (),
    pub res_fuzz_count: FxHashMap<Box<str>, u64>,
    pub call_fuzz_count: FxHashMap<Rc<Syscall>, u64>,
}

pub struct FuzzInstance {
    ls: LocalState,
    gs: Arc<Mutex<GlobalState>>,
}

impl FuzzInstance {
    pub fn new(_: ()) -> Self {
        // init state
        // boot kernel
        // run executor
        // bind current thread to specific cpu core. This is great.
        todo!()
    }

    pub fn run_fuzz(&mut self) {
        // gen & mutate
        // exec
        // analyze result:
        //     normal: update state
        //     hang: update state, dicard test case
        //     crash: extract crash info(title, stack trace, cpu state)
        //            try to repto
        //            save info and related test case
        //            reboot
    }
}
