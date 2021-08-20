//! Mutate value of `integer` like types.
#[cfg(debug_assertions)]
use crate::mutation::call::display_value_diff;
use crate::{
    context::Context,
    gen::int::{gen_flags_bitmask, gen_flags_non_bitmask, gen_int, gen_proc},
    ty::IntType,
    value::Value,
    RngType,
};
use rand::prelude::*;

pub fn mutate_int(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);

    if rng.gen() {
        let new_val = gen_int(ctx, rng, ty, val.dir());
        debug_info!(
            "mutate_int(gen): {}",
            display_value_diff(val, &new_val, ctx.target)
        );
        let mutated = new_val.checked_as_int().val != val.checked_as_int().val;
        *val = new_val;
        return mutated;
    }

    let val = val.checked_as_int_mut();
    let ty = ty.checked_as_int();
    let bit_sz = ty.bit_size();
    let mut new_val = if ty.align() == 0 {
        do_mutate_int(rng, val.val, ty)
    } else {
        do_mutate_aligned_int(rng, val.val, ty)
    };
    if bit_sz < 64 {
        new_val &= (1 << bit_sz) - 1;
    }

    debug_info!("mutate_int: {:#x} -> {:#x}", val.val, new_val);
    let mutated = val.val != new_val;
    val.val = new_val;

    mutated
}

fn do_mutate_int(rng: &mut RngType, old_val: u64, ty: &IntType) -> u64 {
    if rng.gen_ratio(1, 3) {
        old_val.wrapping_add(rng.gen_range(1..=4))
    } else if rng.gen_ratio(1, 2) {
        old_val.wrapping_sub(rng.gen_range(1..=4))
    } else {
        let bit_sz = ty.bit_size();
        old_val ^ (1 << rng.gen_range(0..bit_sz))
    }
}

fn do_mutate_aligned_int(rng: &mut RngType, old_val: u64, ty: &IntType) -> u64 {
    let r = ty.range().cloned().unwrap_or(0..=u64::MAX);
    let start = *r.start();
    let mut end = *r.end();
    if start == 0 && end == u64::MAX {
        end = 1_u64.wrapping_shl(ty.bit_size() as u32).wrapping_sub(1);
    }
    let index = old_val.wrapping_sub(start) / ty.align();
    let miss = old_val.wrapping_sub(start) % ty.align();
    let mut index = do_mutate_int(rng, index, ty);
    let last_index = end.wrapping_sub(start) / ty.align();
    index %= last_index + 1;
    start
        .wrapping_add(index.wrapping_mul(ty.align()))
        .wrapping_add(miss)
}

pub fn mutate_const(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    debug_info!("mutate_const: doing nothing");
    false
}

pub fn mutate_flags(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let mut tries = 0;
    let mut mutated = false;
    let val = val.checked_as_int_mut();
    let ty = val.ty(ctx.target).checked_as_flags();

    let mut new_val = val.val;
    while tries < 128 && !mutated {
        new_val = if ty.bit_mask() {
            gen_flags_bitmask(rng, ty.vals(), val.val)
        } else {
            gen_flags_non_bitmask(rng, ty.vals(), val.val)
        };
        mutated = new_val != val.val;
        tries += 1;
    }

    debug_info!("mutate_flags: {:#b} -> {:#b}", new_val, val.val);
    val.val = new_val;
    mutated
}

pub fn mutate_len(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    let new_val = gen_int(ctx, rng, ty, val.dir());
    debug_info!(
        "mutate_len: {}",
        display_value_diff(val, &new_val, ctx.target)
    );
    *val = new_val;

    false // length mutation actually is meaning less
}

pub fn mutate_proc(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    let new_val = gen_proc(ctx, rng, ty, val.dir());
    debug_info!(
        "mutate_proc: {}",
        display_value_diff(val, &new_val, ctx.target)
    );
    let mutated = new_val.checked_as_int().val != val.checked_as_int().val;
    *val = new_val;

    mutated
}

pub fn mutate_csum(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    let new_val = gen_int(ctx, rng, ty, val.dir());
    debug_info!(
        "mutate_csum: {}",
        display_value_diff(val, &new_val, ctx.target)
    );
    *val = new_val;

    false // csum mutation is meaning less
}
