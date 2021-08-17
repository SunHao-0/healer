//! Mutate value of `ptr`, `vma` type.
use crate::{context::Context, gen::ptr::gen_vma, value::Value, RngType};

pub fn mutate_ptr(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    false
}

pub fn mutate_vma(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    let new_val = gen_vma(ctx, rng, ty, val.dir());
    *val = new_val;
    true
}
