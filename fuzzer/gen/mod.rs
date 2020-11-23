/// resource oriented generation algorithm
use crate::fuzzer::ValuePool;
use crate::target::Target;
use hlang::ast::*;
use rand::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

mod select;

/// Gnerate test case based current value pool and test target.
pub fn gen(target: &Target, pool: &ValuePool) -> Prog {
    let mut ctx = GenContext {
        target,
        generated_res: FxHashMap::default(),
        generated_buf: FxHashMap::default(),
        pool,
    };
    gen_inner(&mut ctx)
}

/// Generated resource during one generation.
type ResPool = FxHashMap<Rc<Type>, FxHashSet<Rc<ResValue>>>;

/// Generation context.
/// A context contains test target, generated resource and buffer value, global value pool.
struct GenContext<'a, 'b> {
    target: &'a Target,
    generated_res: ResPool,
    generated_buf: FxHashMap<Rc<Type>, Value>,
    pool: &'b ValuePool,
}

fn gen_inner(ctx: &mut GenContext) -> Prog {
    let calls: Vec<Call> = Vec::new();
    while !should_stop(calls.len()) {
        let next_sc = select::select_syscall(ctx);
    }
    todo!()
}

fn should_stop(len: usize) -> bool {
    const MIN_LEN: usize = 4;
    const MAX_LEN: usize = 16;
    if len < MIN_LEN {
        // not long enough
        false
    } else if len < MAX_LEN {
        // we can continue, we can alse stop, so we use rand.
        let delta = 0.8 * (len as f32 / MAX_LEN as f32);
        random::<f32>() < delta
    } else {
        true
    }
}

fn gen_call(ctx: &mut GenContext, sc: Rc<Syscall>) -> Call {
    todo!()
}
