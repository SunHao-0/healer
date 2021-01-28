use crate::fuzz::fuzzer::ValuePool;
use crate::gen::{
    call,
    param::{
        self,
        alloc::{MemAlloc, VmaAlloc},
    },
};
use crate::model::*;
use crate::targets::Target;

use rustc_hash::{FxHashMap, FxHashSet};

/// Generated resource during one generation.
type ResPool = FxHashMap<TypeRef, Vec<*mut ResValue>>;

/// Generation context.
/// A context contains test target, generated resource and buffer value, global value pool.
pub(crate) struct GenContext<'a, 'b> {
    pub(crate) target: &'a Target,
    pub(crate) generated_res: ResPool,
    pub(crate) generated_str: FxHashMap<TypeRef, FxHashSet<Box<[u8]>>>,
    pub(crate) pool: &'b ValuePool,
    // id for resource value count.
    pub(crate) id_count: usize,
    pub(crate) mem_alloc: MemAlloc,
    pub(crate) vma_alloc: VmaAlloc,
    // handle recusive type or circle reference.
    pub(crate) rec_depth: FxHashMap<TypeRef, usize>,

    pub(crate) call_ctx: call::GenCallContext,
    pub(crate) param_ctx: param::GenParamContext,
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

    pub fn add_res(&mut self, ty: TypeRef, res: *mut ResValue) {
        let entry = self.generated_res.entry(ty).or_default();
        entry.push(res);
    }

    pub fn add_str(&mut self, ty: TypeRef, new_str: Box<[u8]>) -> bool {
        let entry = self.generated_str.entry(ty).or_default();
        entry.insert(new_str)
    }

    pub fn inc_rec_depth(&mut self, ty: TypeRef) -> usize {
        let entry = self.rec_depth.entry(ty).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn dec_rec_depth(&mut self, ty: TypeRef) {
        if let Some(v) = self.rec_depth.get_mut(&ty) {
            *v -= 1;
        } else {
            return;
        }
        if self.rec_depth[&ty] == 0 {
            self.rec_depth.remove(&ty);
        }
    }

    pub fn record_len_to_call_ctx(&mut self, len: (*mut u64, LenInfo)) {
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

    pub fn generating_syscall(&self) -> Option<&Syscall> {
        self.call_ctx.generating_syscall.as_deref()
    }
}
