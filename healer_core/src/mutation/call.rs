//! Call level mutation.

use rand::Rng;

use super::{
    buffer::{mutate_buffer_blob, mutate_buffer_filename, mutate_buffer_string},
    foreach_call_arg, foreach_call_arg_mut,
    group::{mutate_array, mutate_struct, mutate_union},
    int::{mutate_const, mutate_csum, mutate_flags, mutate_int, mutate_len, mutate_proc},
    ptr::{mutate_ptr, mutate_vma},
    res::mutate_res,
    restore_res_ctx,
};
use crate::{
    context::Context,
    corpus::CorpusWrapper,
    gen::choose_weighted,
    len::calculate_len_call,
    prog::Call,
    target::Target,
    ty::{Dir, Type},
    value::Value,
    RngType,
};

/// Select a call and mutate its args randomly.
pub fn mutate_call_args(ctx: &mut Context, _corpus: &CorpusWrapper, rng: &mut RngType) -> bool {
    if ctx.calls.is_empty() {
        return false;
    }
    if let Some(idx) = select_call(ctx, rng) {
        do_mutate(ctx, rng, idx)
    } else {
        false
    }
}

/// Mutate the `idx` call in `ctx` based on value prio.
fn do_mutate(ctx: &mut Context, rng: &mut RngType, mut idx: usize) -> bool {
    let mut tries = 0;
    let mut mutated = false;

    while tries < 128 && (!mutated || rng.gen_ratio(1, 2)) {
        // restore
        ctx.res_ids.clear();
        ctx.res_kinds.clear();
        ctx.mem_allocator.restore();

        let (args, prios) = collect_args(ctx.target, &mut ctx.calls[idx]);
        if args.is_empty() {
            return false;
        }
        restore_res_ctx(ctx, idx);

        let arg_idx = choose_weighted(rng, &prios);
        // SAFETY: the address of each value is stable before mutation
        let arg = unsafe { args[arg_idx].as_mut().unwrap() };

        let mut calls_backup = std::mem::take(&mut ctx.calls);
        mutated = mutate_value(ctx, rng, arg);
        if !ctx.calls.is_empty() {
            let new_calls = std::mem::take(&mut ctx.calls);
            let new_idx = idx + new_calls.len();
            calls_backup.splice(idx..idx, new_calls);
            idx = new_idx;
        }
        ctx.calls = calls_backup;

        tries += 1;
    }

    if mutated {
        calculate_len_call(ctx.target, &mut ctx.calls[idx]);
    }

    mutated
}

/// Collect args of `call`, return the addresses and corresponding prios
fn collect_args(target: &Target, call: &mut Call) -> (Vec<*mut Value>, Vec<u64>) {
    let mut arg_addrs = Vec::new();
    let mut prios = Vec::new();
    let mut prio_sum = 0;

    foreach_call_arg_mut(call, |val| {
        let prio = val_prio(target, val);
        if prio == NEVER_MUTATE {
            return;
        }

        let ty = val.ty(target);
        let is_buffer_like = ty.as_buffer_blob().is_none()
            || ty.as_buffer_filename().is_none()
            || ty.as_buffer_string().is_none();
        let is_array = ty.as_array().is_none();
        if !is_buffer_like && !is_array && val.dir() == Dir::Out && !ty.varlen() && ty.size() == 0 {
            return;
        }

        prio_sum += prio;
        arg_addrs.push(val as *mut _);
        prios.push(prio_sum);
    });

    (arg_addrs, prios)
}

type TypeMutator = fn(&mut Context, &mut RngType, &mut Value) -> bool;
const TYPE_MUTATORS: [TypeMutator; 15] = [
    mutate_res,
    mutate_const,
    mutate_int,
    mutate_flags,
    mutate_len,
    mutate_proc,
    mutate_csum,
    mutate_vma,
    mutate_buffer_blob,
    mutate_buffer_string,
    mutate_buffer_filename,
    mutate_array,
    mutate_ptr,
    mutate_struct,
    mutate_union,
];

/// Mutate the given value
pub fn mutate_value(ctx: &mut Context, rng: &mut RngType, val: &mut Value) -> bool {
    let ty = val.ty(ctx.target);
    TYPE_MUTATORS[ty.kind() as usize](ctx, rng, val)
}

/// Select a call from `calls` based on specific priority.
///
/// # Panics
/// Panics if the input `calls` is empty.
pub fn select_call(ctx: &mut Context, rng: &mut RngType) -> Option<usize> {
    let mut prios = Vec::with_capacity(ctx.calls.len());
    let mut prio_sum = 0;
    let mut ret = None;

    for call in &ctx.calls {
        let mut prio = 0;
        foreach_call_arg(call, |val| {
            prio += val_prio(ctx.target, val);
        });
        prio_sum += prio;
        prios.push(prio_sum);
    }
    if prio_sum != 0 {
        ret = Some(choose_weighted(rng, &prios));
    }

    ret
}

const NEVER_MUTATE: u64 = 0;
const MIN_PRIO: u64 = 1;
const MID_PRIO: u64 = 5;
const MAX_PRIO: u64 = 10;

const TYPE_PRIOS: [fn(&Type, Dir) -> u64; 15] = [
    res_prio,
    const_prio,
    int_prio,
    flags_prio,
    len_prio,
    proc_prio,
    csum_prio,
    vma_prio,
    buffer_blob_prio,
    buffer_string_prio,
    buffer_filename_prio,
    array_prio,
    ptr_prio,
    struct_prio,
    union_prio,
];

#[inline]
fn val_prio(target: &Target, val: &Value) -> u64 {
    let ty = target.ty_of(val.ty_id());
    TYPE_PRIOS[ty.kind() as usize](ty, val.dir())
}

fn int_prio(ty: &Type, _dir: Dir) -> u64 {
    let ty = ty.checked_as_int();
    let bit_sz = ty.bit_size();
    let plain = (bit_sz as f64).log2() as u64 + 1;
    if ty.range().is_none() {
        return bit_sz;
    }
    let range = ty.range().unwrap();
    let start = *range.start();
    let end = *range.end();
    let mut size = end - start + 1;
    if ty.align() != 0 {
        if start == 0 && end == u64::MAX {
            size = ((1 << bit_sz) - 1) / ty.align() + 1;
        } else {
            size = (end - start) / ty.align() + 1;
        }
    }
    if size <= 15 {
        range_size_prio(size)
    } else if size <= 256 {
        MAX_PRIO
    } else {
        plain
    }
}

#[inline]
fn range_size_prio(size: u64) -> u64 {
    match size {
        0 => NEVER_MUTATE,
        1 => MIN_PRIO,
        _ => std::cmp::min(size / 3 + 4, 9),
    }
}

#[inline]
fn const_prio(_ty: &Type, _dir: Dir) -> u64 {
    NEVER_MUTATE
}

#[inline]
fn flags_prio(ty: &Type, _dir: Dir) -> u64 {
    let ty = ty.checked_as_flags();
    let mut p = range_size_prio(ty.vals().len() as u64);
    if ty.bit_mask() {
        p += 1;
    }
    p
}

#[inline]
fn csum_prio(_ty: &Type, _dir: Dir) -> u64 {
    NEVER_MUTATE
}

#[inline]
fn len_prio(_ty: &Type, _dir: Dir) -> u64 {
    NEVER_MUTATE
}

#[inline]
fn proc_prio(_ty: &Type, _dir: Dir) -> u64 {
    MID_PRIO
}

#[inline]
fn res_prio(_ty: &Type, _dir: Dir) -> u64 {
    MID_PRIO
}

#[inline]
fn vma_prio(_ty: &Type, _dir: Dir) -> u64 {
    MID_PRIO
}

#[inline]
fn ptr_prio(_ty: &Type, _dir: Dir) -> u64 {
    MID_PRIO
}

#[inline]
fn buffer_blob_prio(ty: &Type, dir: Dir) -> u64 {
    if dir == Dir::Out && !ty.varlen() {
        NEVER_MUTATE
    } else {
        8
    }
}

#[inline]
fn buffer_string_prio(ty: &Type, dir: Dir) -> u64 {
    let ty = ty.checked_as_buffer_string();
    if dir == Dir::Out && !ty.varlen() || ty.vals().len() == 1 {
        NEVER_MUTATE
    } else {
        8
    }
}

#[inline]
fn buffer_filename_prio(ty: &Type, dir: Dir) -> u64 {
    if dir == Dir::Out && !ty.varlen() {
        NEVER_MUTATE
    } else {
        8
    }
}

#[inline]
fn array_prio(ty: &Type, _dir: Dir) -> u64 {
    let ty = ty.checked_as_array();
    if let Some(range) = ty.range() {
        if *range.start() == *range.end() {
            return NEVER_MUTATE;
        }
    }
    MAX_PRIO
}

#[inline]
fn struct_prio(_ty: &Type, _dir: Dir) -> u64 {
    NEVER_MUTATE
}

#[inline]
fn union_prio(ty: &Type, _dir: Dir) -> u64 {
    let ty = ty.checked_as_union();
    if ty.fields().len() == 1 {
        NEVER_MUTATE
    } else {
        MAX_PRIO
    }
}
