//! Mutate value of `integer` like types.
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
        *val = gen_int(ctx, rng, ty, val.dir());
        return true;
    }

    let val = val.checked_as_int_mut();
    let ty = ty.checked_as_int();
    let bit_sz = ty.bit_size();
    val.val = if ty.align() == 0 {
        do_mutate_int(rng, val.val, ty)
    } else {
        do_mutate_aligned_int(rng, val.val, ty)
    };

    val.val &= (1 << bit_sz) - 1;

    true
}

fn do_mutate_int(rng: &mut RngType, old_val: u64, ty: &IntType) -> u64 {
    if rng.gen_ratio(1, 3) {
        let (new_val, _) = old_val.overflowing_add(rng.gen_range(1..=4));
        new_val
    } else if rng.gen_ratio(1, 2) {
        let (new_val, _) = old_val.overflowing_sub(rng.gen_range(1..=4));
        new_val
    } else {
        let bit_sz = ty.bit_size();
        old_val ^ (1 << rng.gen_range(0..bit_sz))
    }
}

fn do_mutate_aligned_int(rng: &mut RngType, old_val: u64, ty: &IntType) -> u64 {
    let start = *ty.range().unwrap().start();
    let mut end = *ty.range().unwrap().end();
    if start == 0 && end == u64::MAX {
        end = (1 << ty.bit_size()) - 1;
    }
    let index = (old_val - start) / ty.align();
    let miss = (old_val - start) % ty.align();
    let mut index = do_mutate_int(rng, index, ty);
    let last_index = (end - start) / ty.align();
    index %= last_index + 1;
    start + index * ty.align() + miss
}

pub fn mutate_const(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    false
}

pub fn mutate_flags(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let mut tries = 0;
    let mut mutated = false;
    let val = val.checked_as_int_mut();
    let ty = val.ty(ctx.target).checked_as_flags();

    while tries < 128 && !mutated {
        let new_val = if ty.bit_mask() {
            gen_flags_bitmask(rng, ty.vals(), val.val)
        } else {
            gen_flags_non_bitmask(rng, ty.vals(), val.val)
        };
        mutated = new_val != val.val;
        val.val = new_val;
        tries += 1;
    }

    mutated
}

pub fn mutate_len(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    false
}

pub fn mutate_proc(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    *val = gen_proc(ctx, rng, ty, val.dir());
    true
}

pub fn mutate_csum(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) -> bool {
    false
}
