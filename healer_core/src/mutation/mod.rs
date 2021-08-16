//! Prog mutation.
use crate::{
    context::Context,
    corpus::CorpusWrapper,
    gen::{choose_weighted, gen_one_call, prog_len_range},
    prog::{Call, Prog},
    relation::Relation,
    select::select_with_calls,
    syscall::SyscallId,
    target::Target,
    ty::{Dir, TypeKind},
    value::{ResValueKind, Value, ValueKind},
    HashMap, RngType,
};
use rand::prelude::*;

pub mod buffer;
pub mod group;
pub mod int;
pub mod ptr;
pub mod res;

/// Muate input prog `p`
pub fn mutate(
    target: &Target,
    relation: &Relation,
    corpus: &CorpusWrapper,
    rng: &mut RngType,
    p: &mut Prog,
) {
    type MutateOperation = fn(&mut Context, &CorpusWrapper, &mut RngType) -> bool;
    const OPERATIONS: [MutateOperation; 3] = [insert_calls, mutate_call_args, splice];
    const WEIGHTS: [u64; 3] = [40, 98, 100];

    let calls = std::mem::take(&mut p.calls);
    let mut ctx = Context::new(target, relation);
    restore_partial_ctx(&mut ctx, &calls);
    ctx.calls = calls;

    let mut mutated = false;
    let mut tries = 0;
    while tries < 128
        && ctx.calls.len() < prog_len_range().end
        && (!mutated || ctx.calls.is_empty() || rng.gen_ratio(1, 3))
    {
        // clear mem usage, fixup later
        ctx.mem_allocator.restore();
        // clear newly added res
        ctx.res_ids.clear();
        ctx.res_kinds.clear();

        let idx = choose_weighted(rng, &WEIGHTS);
        mutated = OPERATIONS[idx](&mut ctx, corpus, rng);
        tries += 1;
    }

    fixup_ptr_addr(&mut ctx);
    *p = ctx.to_prog();
}

/// Select a prog from `corpus` and splice it with calls in the `ctx` randomly.
pub fn splice(ctx: &mut Context, corpus: &CorpusWrapper, rng: &mut RngType) -> bool {
    if ctx.calls.is_empty() || ctx.calls.len() > prog_len_range().end || corpus.is_empty() {
        return false;
    }

    let mut calls = corpus.select_one(rng).unwrap().calls;
    mapping_res_id(ctx, &mut calls); // mapping resource id of `calls`, continue with current `ctx.next_res_id`
    restore_partial_ctx(ctx, &calls);
    let idx = rng.gen_range(0..=ctx.calls.len());
    ctx.calls.splice(idx..idx, calls);
    true
}

/// Insert calls to random location of ctx's calls.
pub fn insert_calls(ctx: &mut Context, _corpus: &CorpusWrapper, rng: &mut RngType) -> bool {
    if ctx.calls.len() > prog_len_range().end {
        return false;
    }
    let idx = rng.gen_range(0..=ctx.calls.len());
    restore_res_ctx(ctx, idx); // restore the resource information before call `idx`
    let sid = select_call_to(ctx, rng, idx);
    let mut calls_backup = std::mem::take(&mut ctx.calls);
    gen_one_call(ctx, rng, sid);
    let new_calls = std::mem::take(&mut ctx.calls);
    calls_backup.splice(idx..idx, new_calls);
    ctx.calls = calls_backup;
    true
}

/// Select new call to location `idx`.
fn select_call_to(ctx: &mut Context, rng: &mut RngType, idx: usize) -> SyscallId {
    let mut candidates: HashMap<SyscallId, u64> = HashMap::new();
    let r = ctx.relation();
    let calls = ctx.calls();

    // first, consider calls that can be influenced by calls before `idx`.
    for sid in calls[..idx].iter().map(|c| c.sid()) {
        for candidate in r.influence_of(sid).iter().copied() {
            let entry = candidates.entry(candidate).or_default();
            *entry += 1;
        }
    }

    // then, consider calls that can be influence calls after `idx`.
    if idx != calls.len() {
        for sid in calls[idx..].iter().map(|c| c.sid()) {
            for candidate in r.influence_by_of(sid).iter().copied() {
                let entry = candidates.entry(candidate).or_default();
                *entry += 1;
            }
        }
    }

    let candidates: Vec<(SyscallId, u64)> = candidates.into_iter().collect();
    if let Ok(candidate) = candidates.choose_weighted(rng, |candidate| candidate.1) {
        candidate.0
    } else {
        // failed to select with relation, use normal strategy.
        select_with_calls(ctx, rng)
    }
}

/// Select a call and mutate its args randomly.
pub fn mutate_call_args(_ctx: &mut Context, _corpus: &CorpusWrapper, _rng: &mut RngType) -> bool {
    todo!()
}

/// Restore used vma, generated filenames, generated strs, next res id.  
fn restore_partial_ctx(ctx: &mut Context, calls: &[Call]) {
    for call in calls {
        for arg in call.args() {
            restore_partial_ctx_inner(ctx, arg)
        }
        if let Some(ret) = call.ret.as_ref() {
            restore_partial_ctx_inner(ctx, ret);
        }
    }
}

fn restore_partial_ctx_inner(ctx: &mut Context, val: &Value) {
    match val.kind() {
        ValueKind::Ptr => {
            let val = val.checked_as_ptr();
            if let Some(pointee) = val.pointee.as_ref() {
                restore_partial_ctx_inner(ctx, pointee);
            }
        }
        ValueKind::Vma => {
            let val = val.checked_as_vma();
            let idx = val.addr / ctx.target().page_sz();
            let sz = val.vma_size / ctx.target().page_sz();
            ctx.vma_allocator.note_alloc(idx, sz);
        }
        ValueKind::Data => {
            let val = val.checked_as_data();
            if val.dir() != Dir::In || val.data.len() < 3 {
                return;
            }
            let ty = val.ty(ctx.target);
            match ty.kind() {
                TypeKind::BufferFilename => {
                    ctx.record_filename(&val.data);
                }
                TypeKind::BufferString => ctx.record_str(val.data.clone()),
                _ => (),
            }
        }
        ValueKind::Group => {
            let val = val.checked_as_group();
            for v in &val.inner {
                restore_partial_ctx_inner(ctx, v);
            }
        }
        ValueKind::Union => {
            let val = val.checked_as_union();
            restore_partial_ctx_inner(ctx, &val.option);
        }
        ValueKind::Res => {
            let val = val.checked_as_res();
            if let Some(id) = val.res_val_id() {
                if ctx.next_res_id <= id {
                    ctx.next_res_id = id + 1;
                }
            }
        }
        _ => (),
    }
}

/// Restore generated resources to `ctx`.  
fn restore_res_ctx(ctx: &mut Context, to: usize) {
    let calls_backup = std::mem::take(&mut ctx.calls);
    for c in &calls_backup[0..to] {
        for val in c.args() {
            restore_res_ctx_inner(ctx, val);
        }
        if let Some(ret) = c.ret.as_ref() {
            restore_res_ctx_inner(ctx, ret);
        }
    }
    ctx.calls = calls_backup;
}

fn restore_res_ctx_inner(ctx: &mut Context, val: &Value) {
    match val.kind() {
        ValueKind::Ptr => {
            let val = val.checked_as_ptr();
            if let Some(pointee) = val.pointee.as_ref() {
                restore_res_ctx_inner(ctx, pointee)
            }
        }
        ValueKind::Group => {
            let val = val.checked_as_group();
            for v in &val.inner {
                restore_res_ctx_inner(ctx, v);
            }
        }
        ValueKind::Union => {
            let val = val.checked_as_union();
            restore_res_ctx_inner(ctx, &val.option);
        }
        ValueKind::Res => {
            let val = val.checked_as_res();
            let ty = val.ty(ctx.target()).checked_as_res();
            let kind = ty.res_name();
            if val.dir() != Dir::In && val.is_res() {
                ctx.record_res(kind, val.res_val_id().unwrap());
            }
        }
        _ => (),
    }
}

/// Mapping resource id of `calls`, make sure all `res_id` in `calls` is bigger then current `next_res_id`
fn mapping_res_id(ctx: &mut Context, calls: &mut [Call]) {
    for c in calls {
        for arg in c.args_mut() {
            mapping_res_id_inner(ctx, arg);
        }
        if let Some(ret) = c.ret.as_mut() {
            mapping_res_id_inner(ctx, ret);
        }
    }
}

fn mapping_res_id_inner(ctx: &mut Context, val: &mut Value) {
    match val.kind() {
        ValueKind::Ptr => {
            let val = val.checked_as_ptr_mut();
            if let Some(pointee) = val.pointee.as_mut() {
                mapping_res_id_inner(ctx, pointee)
            }
        }
        ValueKind::Group => {
            let val = val.checked_as_group_mut();
            for v in &mut val.inner {
                mapping_res_id_inner(ctx, v);
            }
        }
        ValueKind::Union => {
            let val = val.checked_as_union_mut();
            mapping_res_id_inner(ctx, &mut val.option);
        }
        ValueKind::Res => {
            let val = val.checked_as_res_mut();
            match &mut val.kind {
                ResValueKind::Ref(id) | ResValueKind::Own(id) => *id += ctx.next_res_id,
                ResValueKind::Null => (),
            }
        }
        _ => (),
    }
}

/// Fixup the addr of ptr value.
pub fn fixup_ptr_addr(ctx: &mut Context) {
    let mut calls_backup = std::mem::take(&mut ctx.calls);
    for call in &mut calls_backup {
        for arg in call.args_mut() {
            fixup_ptr_addr_inner(ctx, arg)
        }
    }
}

fn fixup_ptr_addr_inner(ctx: &mut Context, val: &mut Value) {
    match val.kind() {
        ValueKind::Ptr => {
            let val = val.checked_as_ptr_mut();
            if let Some(pointee) = val.pointee.as_mut() {
                let addr = ctx.mem_allocator.alloc(pointee.layout(ctx.target()));
                val.addr = addr;
                fixup_ptr_addr_inner(ctx, pointee)
            }
        }
        ValueKind::Group => {
            let val = val.checked_as_group_mut();
            for v in &mut val.inner {
                fixup_ptr_addr_inner(ctx, v);
            }
        }
        ValueKind::Union => {
            let val = val.checked_as_union_mut();
            fixup_ptr_addr_inner(ctx, &mut val.option);
        }
        _ => (),
    }
}

/// Mutate the given value
pub fn mutate_value(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) {}
