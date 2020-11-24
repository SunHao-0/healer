/// resource oriented generation algorithm
use crate::fuzz::ValuePool;
use crate::target::Target;
use hlang::ast::*;
use rand::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

// Syscall selection.
mod select;
// Argument value generation.
mod param;
// Call generation.
mod call;

/// Gnerate test case based current value pool and test target.
pub fn gen(target: &Target, pool: &ValuePool) -> Prog {
    let mut ctx = GenContext::new(target, pool);
    gen_inner(&mut ctx)
}

/// Generated resource during one generation.
type ResPool = FxHashMap<Rc<Type>, FxHashSet<Rc<ResValue>>>;

/// Generation context.
/// A context contains test target, generated resource and buffer value, global value pool.
struct GenContext<'a, 'b> {
    target: &'a Target,
    generated_res: ResPool,
    generated_str: FxHashMap<Rc<Type>, FxHashSet<Box<str>>>,
    pool: &'b ValuePool,
    // id for resource value count.
    id_count: usize,
}

impl<'a, 'b> GenContext<'a, 'b> {
    pub fn new(target: &'a Target, pool: &'b ValuePool) -> Self {
        GenContext {
            target,
            generated_res: FxHashMap::default(),
            generated_src: FxHashMap::default(),
            pool,
            id_count: 0,
        }
    }

    pub fn next_id(&mut self) -> usize {
        let id = self.id_count;
        self.id_count += 1;
        id
    }

    pub fn add_res(&mut self, ty: Rc<Type>, res: Rc<ResValue>) -> bool {
        let entry = self.generated_res.entry(ty).or_default();
        entry.insert(res)
    }

    pub fn add_str(&mut self, ty: Rc<Type>, new_str: Box<str>) -> bool {
        let entry = self.generated_str.entry(ty).or_default();
        entry.insert(new_str)
    }
}

fn gen_inner(ctx: &mut GenContext) -> Prog {
    let mut calls: Vec<Call> = Vec::new();
    while !should_stop(calls.len()) {
        let next_syscall = select::select_syscall(ctx);
        calls.push(call::gen(ctx, next_syscall));
    }
    Prog::new(calls)
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
