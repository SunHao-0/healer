use self::param::alloc::{MemAlloc, VmaAlloc};
/// resource oriented generation algorithm
use crate::fuzz::ValuePool;
use crate::model::*;
use crate::targets::Target;
use rand::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

// Syscall selection.
mod select;
// Argument value generation.
mod param;
// Call generation.
mod call;
// Length type calculation. It's here because length calculation is inter-params.
mod len;

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
    generated_str: FxHashMap<Rc<Type>, FxHashSet<Box<[u8]>>>, // use u8 instread of str type for convenience.
    pool: &'b ValuePool,
    // id for resource value count.
    id_count: usize,
    mem_alloc: MemAlloc,
    vma_alloc: VmaAlloc,
    // handle recusive type or circle reference.
    rec_depth: FxHashMap<Rc<Type>, usize>,

    call_ctx: call::GenCallContext,
    param_ctx: param::GenParamContext,
}

impl<'a, 'b> GenContext<'a, 'b> {
    pub fn new(target: &'a Target, pool: &'b ValuePool) -> Self {
        GenContext {
            target,
            generated_res: FxHashMap::default(),
            generated_str: FxHashMap::default(),
            pool,
            id_count: 0,
            mem_alloc: MemAlloc::with_mem_size(target.page_sz * target.page_num),
            vma_alloc: VmaAlloc::with_page_num(target.page_num),
            rec_depth: FxHashMap::default(),
            call_ctx: Default::default(),
            param_ctx: Default::default(),
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

    pub fn add_str(&mut self, ty: Rc<Type>, new_str: Box<[u8]>) -> bool {
        let entry = self.generated_str.entry(ty).or_default();
        entry.insert(new_str)
    }

    pub fn inc_rec_depth(&mut self, ty: &Rc<Type>) -> usize {
        let entry = self.rec_depth.entry(Rc::clone(ty)).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn dec_rec_depth(&mut self, ty: &Rc<Type>) {
        if let Some(v) = self.rec_depth.get_mut(ty) {
            *v -= 1;
        } else {
            return;
        }
        if self.rec_depth[ty] == 0 {
            self.rec_depth.remove(ty);
        }
    }

    pub fn record_len_to_call_ctx(&mut self, len: (*mut u64, Rc<LenInfo>)) {
        self.call_ctx.left_len_vals.push(len);
    }

    pub fn has_len_call_ctx(&self) -> bool {
        !self.call_ctx.left_len_vals.is_empty()
    }

    pub fn record_len_to_param_ctx(&mut self) {
        self.param_ctx.len_type_count += 1;
    }

    pub fn has_len_param_ctx(&self) -> bool {
        self.param_ctx.len_type_count != 0
    }

    pub fn get_generating_syscall(&self) -> Option<&Syscall> {
        self.call_ctx.generating_syscall.as_deref()
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
