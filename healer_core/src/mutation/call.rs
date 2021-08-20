//! Call level mutation.
use super::{
    buffer::{mutate_buffer_blob, mutate_buffer_filename, mutate_buffer_string},
    foreach_call_arg, foreach_call_arg_mut, foreach_value,
    group::{mutate_array, mutate_struct, mutate_union},
    int::{mutate_const, mutate_csum, mutate_flags, mutate_int, mutate_len, mutate_proc},
    ptr::{mutate_ptr, mutate_vma},
    res::mutate_res,
    restore_res_ctx,
};
use crate::{
    context::Context,
    corpus::CorpusWrapper,
    gen::{choose_weighted, current_builder, pop_builder, push_builder},
    len::calculate_len_call,
    prog::Call,
    target::Target,
    ty::{Dir, ResKind, Type},
    value::{ResValueId, Value, ValueKind},
    HashMap, HashSet, RngType,
};
use rand::Rng;

/// Select a call and mutate its args randomly.
pub fn mutate_call_args(ctx: &mut Context, _corpus: &CorpusWrapper, rng: &mut RngType) -> bool {
    if ctx.calls.is_empty() {
        return false;
    }
    if let Some(idx) = select_call(ctx, rng) {
        do_mutate_call_args(ctx, rng, idx)
    } else {
        debug_info!("mutate_call_args: all calls do not have mutable parameters");
        false
    }
}

/// Mutate the `idx` call in `ctx` based on value prio.
fn do_mutate_call_args(ctx: &mut Context, rng: &mut RngType, mut idx: usize) -> bool {
    let mut tries = 0;
    let mut mutated = false;
    let mut visited = HashSet::new();

    while tries < 128 && (!mutated || rng.gen_ratio(1, 2)) {
        // restore
        ctx.res_ids.clear();
        ctx.res_kinds.clear();
        ctx.mem_allocator.restore();

        let arg = if let Some(arg) = select_arg(ctx, rng, idx) {
            arg
        } else {
            return false;
        };
        if !visited.insert(arg as *const _) && rng.gen_ratio(9, 10) {
            continue;
        }

        restore_res_ctx(ctx, idx);
        record_call_res(&ctx.calls[idx]);
        let mut calls_backup = std::mem::take(&mut ctx.calls);
        mutated = mutate_value(ctx, rng, arg);
        restore_call_res(&mut calls_backup[idx]);

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

fn record_call_res(call: &Call) {
    push_builder(call.sid());
    current_builder(|b| {
        for (res, ids) in call.generated_res.clone() {
            b.generated_res.insert(res, ids.into_iter().collect());
        }
        for (res, ids) in call.used_res.clone() {
            b.used_res.insert(res, ids.into_iter().collect());
        }
    });
}

fn restore_call_res(call: &mut Call) {
    let b = pop_builder();
    let mut ge: HashMap<ResKind, Vec<ResValueId>> = HashMap::default();
    for (res, ids) in b.generated_res {
        ge.insert(res, ids.into_iter().collect());
    }
    let mut ue: HashMap<ResKind, Vec<ResValueId>> = HashMap::default();
    for (res, ids) in b.used_res {
        ue.insert(res, ids.into_iter().collect());
    }
    call.generated_res = ge;
    call.used_res = ue;
}

fn select_arg(ctx: &mut Context, rng: &mut RngType, idx: usize) -> Option<&'static mut Value> {
    let (args, prios) = collect_args(ctx.target, &mut ctx.calls[idx]);
    if args.is_empty() {
        debug_info!("do_mutate_call_args: no mutable args");
        return None;
    }

    let arg_idx = choose_weighted(rng, &prios);
    // SAFETY: the address of each value is stable before mutation
    let arg = unsafe { args[arg_idx].as_mut().unwrap() };
    debug_info!(
        "do_mutate_call_args: mutating {} of {} args, ty: {}, prio: {}",
        arg_idx,
        args.len(),
        arg.ty(ctx.target),
        if arg_idx == 0 {
            prios[arg_idx]
        } else {
            prios[arg_idx] - prios[arg_idx - 1]
        }
    );
    Some(arg)
}

/// Collect args of `call`, return the addresses and corresponding prios
fn collect_args(target: &Target, call: &mut Call) -> (Vec<*mut Value>, Vec<u64>) {
    let mut arg_addrs = Vec::new();
    let mut prios = Vec::new();
    let mut prio_sum = 0;

    foreach_call_arg_mut(call, |val| {
        let prio = val_prio(target, val);
        if prio == NEVER_MUTATE || !val_mutable(target, val) {
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
        let idx = choose_weighted(rng, &prios);
        let _syscall = ctx.target.syscall_of(ctx.calls[idx].sid());
        debug_info!(
            "select_call: prios: {:?}",
            prios.iter().enumerate().collect::<Vec<_>>()
        );
        debug_info!(
            "select_call: call-{} {} selected, prio: {}",
            idx,
            _syscall.name(),
            if idx == 0 {
                prios[idx]
            } else {
                prios[idx] - prios[idx - 1]
            }
        );
        ret = Some(idx);
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

/// Derive if a value is mutable based on type information and value direction.
fn val_mutable(target: &Target, val: &Value) -> bool {
    use super::TypeKind::*;
    let ty = val.ty(target);

    // ignore zero-sized type
    if !ty.varlen() && ty.size() == 0 {
        return false;
    }

    // only buffer&array type with varlen is mutable, when dir equals to `Out`
    if val.dir() == Dir::Out {
        match ty.kind() {
            BufferString | BufferBlob | BufferFilename => ty.varlen(),
            Array => {
                let ty = ty.checked_as_array();
                let mut ret = true;
                if let Some(r) = ty.range() {
                    ret = *r.start() != *r.end()
                }
                ret
            }
            _ => false,
        }
    } else {
        // Do not mutate const, len, csum
        // For struct. mutate inner fields instead of strutc itself
        !matches!(ty.kind(), Const | Len | Csum | Struct)
    }
}

#[inline]
fn val_prio(target: &Target, val: &Value) -> u64 {
    let ty = target.ty_of(val.ty_id());
    if !val_mutable(target, val) {
        NEVER_MUTATE
    } else {
        TYPE_PRIOS[ty.kind() as usize](ty, val.dir())
    }
}

fn int_prio(ty: &Type, _dir: Dir) -> u64 {
    let ty = ty.checked_as_int();
    let bit_sz = ty.bit_size();
    let plain = (bit_sz as f64).log2() as u64 + 1;
    if ty.range().is_none() {
        return plain;
    }
    let range = ty.range().unwrap();
    let start = *range.start();
    let end = *range.end();
    let mut size = end.wrapping_sub(start).wrapping_add(1);
    if ty.align() != 0 {
        if start == 0 && end == u64::MAX {
            size = (1_u64.wrapping_shl(bit_sz as u32).wrapping_sub(1) / ty.align()).wrapping_add(1)
        } else {
            size = (end.wrapping_sub(start) / ty.align()).wrapping_add(1);
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
    MIN_PRIO
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
    NEVER_MUTATE // struct contains fix number of fields, so mutate its fields instead of iteself
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

#[inline]
pub(crate) fn contains_out_res(val: &Value) -> bool {
    contain_res(val, Dir::In)
}

fn contain_res(val: &Value, exclude_dir: Dir) -> bool {
    let mut ret = false;
    foreach_value(val, |v| {
        if let Some(res) = v.as_res() {
            if res.dir() != exclude_dir {
                ret = true;
            }
        }
    });
    ret
}

/// Verbose the diff between value with same type.
#[allow(dead_code)] // only called in not release build mode.
pub(crate) fn display_value_diff(old_val: &Value, new_val: &Value, target: &Target) -> String {
    use ValueKind::*;

    match old_val.kind() {
        Integer | Vma | Res => {
            format!("{} -> {}", old_val.display(target), new_val.display(target))
        }
        ValueKind::Ptr => {
            let old_ptr = old_val.checked_as_ptr();
            let new_ptr = new_val.checked_as_ptr();
            let old = if old_ptr.pointee.is_none() {
                "nil".to_string()
            } else {
                format!("{:#x}", old_ptr.addr)
            };
            let new = if new_ptr.pointee.is_none() {
                "nil".to_string()
            } else {
                format!("{:#x}", old_ptr.addr)
            };
            format!("{} -> {}", old, new)
        }
        ValueKind::Data => {
            format!("{} -> {}", old_val.size(target), new_val.size(target))
        }
        ValueKind::Group => {
            let old = old_val.checked_as_group();
            let new = new_val.checked_as_group();
            format!("{} -> {}", old.inner.len(), new.inner.len())
        }
        ValueKind::Union => {
            let old = old_val.checked_as_union();
            let new = new_val.checked_as_union();
            format!("{} -> {}", old.index, new.index)
        }
    }
}
