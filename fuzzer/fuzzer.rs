use std::sync::{Arc, Mutex};

pub struct GlobalState {
    branches: (),
    covers: (),
    corpus: (),
}

pub struct LocalState {
    branches: (),
    cover: (),
}

pub struct FuzzInstance {
    ls: LocalState,
    gs: Arc<Mutex<GlobalState>>,
}

impl FuzzInstance {
    pub fn new() -> Self {
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
