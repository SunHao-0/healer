//! Mutate value of `array`, `struct`, `union` type.
use super::call::contains_out_res;
use crate::{context::Context, gen::gen_ty_value, value::Value, RngType};
use rand::prelude::*;

#[allow(clippy::comparison_chain)]
pub fn mutate_array(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target).checked_as_array();
    let val = val.checked_as_group_mut();

    let old_len = val.inner.len();
    let mut new_len = old_len;
    if let Some(r) = ty.range() {
        let mut changed = false;
        while *r.start() != *r.end() && !changed {
            new_len = rng.gen_range(r.clone()) as usize;
            changed = new_len != old_len;
        }
    } else if rng.gen() {
        while new_len == old_len || rng.gen() {
            new_len += 1;
        }
    } else {
        new_len = rng.gen_range(0..=10)
    };

    let mut shuffled = false;
    if new_len > old_len {
        let new_vals = (0..new_len - old_len)
            .map(|_| gen_ty_value(ctx, rng, ty.elem(), val.dir()))
            .collect::<Vec<_>>();
        val.inner.extend(new_vals);
    } else if new_len < old_len {
        if val.inner.iter().all(|v| !contains_out_res(v)) {
            val.inner.drain(new_len..);
        }
    } else {
        val.inner.shuffle(rng);
        shuffled = true;
    }
    verbose!("mutate_array: {} -> {}", old_len, val.inner.len());

    old_len != val.inner.len() || shuffled
}

pub fn mutate_struct(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    verbose!("mutate_struct: doing nothing");
    false
}

pub fn mutate_union(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let union_val = val.checked_as_union_mut();
    let ty = union_val.ty(ctx.target).checked_as_union();

    if ty.fields().len() <= 1 || contains_out_res(&union_val.option) {
        verbose!("mutate_union: fields too short or contains output res, skip");
        return false;
    }

    let old_index = union_val.index as usize;
    let mut new_index = old_index;
    while new_index == old_index {
        new_index = rng.gen_range(0..ty.fields().len());
    }
    let f = &ty.fields()[new_index];
    let dir = f.dir().unwrap_or_else(|| union_val.option.dir());
    let new_val = gen_ty_value(ctx, rng, f.ty(), dir);
    union_val.option = Box::new(new_val);
    union_val.index = new_index as u64;
    verbose!("mutate_union: index {} -> {}", old_index, new_index);

    true
}
