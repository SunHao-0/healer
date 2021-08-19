//! Mutate value of `ptr`, `vma` type.
use super::call::contains_out_res;
#[cfg(debug_assertions)]
use super::call::display_value_diff;
use crate::{
    context::Context,
    gen::ptr::gen_vma,
    value::{PtrValue, Value},
    RngType,
};
use rand::Rng;

pub fn mutate_ptr(_ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    if contains_out_res(val) {
        return false;
    }

    if rng.gen_ratio(1, 1000) {
        // set null
        let new_val = PtrValue::new_special(val.ty_id(), val.dir(), 0).into();
        verbose!(
            "mutate_ptr: {}",
            display_value_diff(val, &new_val, _ctx.target)
        );
        *val = new_val;
        return true;
    }

    false
}

pub fn mutate_vma(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    let new_val = gen_vma(ctx, rng, ty, val.dir());
    verbose!(
        "mutate_vma: {}",
        display_value_diff(val, &new_val, ctx.target)
    );
    *val = new_val;

    true
}
