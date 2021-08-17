//! Mutate value of `array`, `struct`, `union` type.
use crate::{context::Context, gen::gen_ty_value, value::Value, RngType};
use rand::prelude::*;

#[allow(clippy::comparison_chain)]
pub fn mutate_array(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target).checked_as_array();
    let val = val.checked_as_group_mut();
    let old_len = val.inner.len();

    let new_len = if let Some(r) = ty.range() {
        let mut changed = false;
        let mut new_len = old_len;
        while *r.start() != *r.end() && !changed {
            new_len = rng.gen_range(r.clone()) as usize;
            changed = new_len != old_len;
        }
        new_len
    } else if rng.gen() {
        let mut new_len = old_len;
        while new_len == old_len || rng.gen() {
            new_len += 1;
        }
        new_len
    } else {
        rng.gen_range(0..=10)
    };

    if new_len > old_len {
        let new_vals = (0..new_len - old_len)
            .map(|_| gen_ty_value(ctx, rng, ty.elem(), val.dir()))
            .collect::<Vec<_>>();
        val.inner.extend(new_vals);
    } else if new_len < old_len {
        val.inner.drain(new_len..);
        // TODO fix removed res
    }

    old_len != new_len
}

pub fn mutate_struct(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    false
}

pub fn mutate_union(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let union_val = val.checked_as_union_mut();
    let ty = union_val.ty(ctx.target).checked_as_union();

    if ty.fields().len() <= 1 {
        return false;
    }
    let old_index = union_val.index as usize;
    let mut new_index = old_index;
    while new_index == old_index {
        new_index = rng.gen_range(0..ty.fields().len());
    }
    let f = &ty.fields()[new_index];
    let new_val = gen_ty_value(ctx, rng, f.ty(), f.dir().unwrap_or_else(|| val.dir()));
    *val = new_val;
    true
}
