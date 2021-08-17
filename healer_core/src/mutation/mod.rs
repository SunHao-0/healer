//! Prog mutation.
use crate::{
    context::Context,
    corpus::CorpusWrapper,
    gen::{choose_weighted, gen_one_call, prog_len_range},
    len::calculate_len_call,
    prog::{Call, Prog},
    relation::Relation,
    select::select_with_calls,
    syscall::SyscallId,
    target::Target,
    ty::{Dir, TypeKind},
    value::{ResValueId, ResValueKind, Value, ValueKind},
    verbose, HashMap, RngType,
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
        && (!mutated || ctx.calls.is_empty() || rng.gen_ratio(1, 2))
    {
        let idx = choose_weighted(rng, &WEIGHTS);
        if verbose() {
            log::info!("using strategy-{}", idx);
        }
        mutated = OPERATIONS[idx](&mut ctx, corpus, rng);
        tries += 1;

        // clear mem usage, fixup later
        ctx.mem_allocator.restore();
        // clear newly added res
        ctx.res_ids.clear();
        ctx.res_kinds.clear();
    }

    fixup(&mut ctx); // fixup ptr address, make res id continuous, calculate size
    *p = ctx.to_prog();
}

/// Select a prog from `corpus` and splice it with calls in the `ctx` randomly.
pub fn splice(ctx: &mut Context, corpus: &CorpusWrapper, rng: &mut RngType) -> bool {
    if ctx.calls.is_empty() || ctx.calls.len() > prog_len_range().end || corpus.is_empty() {
        return false;
    }

    let p = corpus.select_one(rng).unwrap();
    if verbose() {
        log::info!(
            "splice: splicing following prog:\n{}",
            p.display(ctx.target)
        );
    }
    let mut calls = p.calls;
    // mapping resource id of `calls`, continue with current `ctx.next_res_id`
    mapping_res_id(ctx, &mut calls);
    restore_partial_ctx(ctx, &calls);
    let idx = rng.gen_range(0..=ctx.calls.len());
    if verbose() {
        log::info!(
            "splice: splicing {} call(s) to location {}",
            calls.len(),
            idx
        );
    }
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
    if verbose() {
        log::info!(
            "insert_calls: inserting {} to location {}",
            ctx.target.syscall_of(sid),
            idx
        );
    }
    let mut calls_backup = std::mem::take(&mut ctx.calls);
    gen_one_call(ctx, rng, sid);
    let new_calls = std::mem::take(&mut ctx.calls);
    if verbose() {
        log::info!("insert_calls: {} call(s) inserted", new_calls.len());
    }
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
    false
}

/// Mutate the given value
pub fn mutate_value(_ctx: &mut Context, _rng: &mut RngType, _val: &mut Value) {}

/// Restore used vma, generated filenames, generated strs, next res id.  
fn restore_partial_ctx(ctx: &mut Context, calls: &[Call]) {
    for call in calls {
        foreach_call_arg(call, |val| {
            if let Some(val) = val.as_vma() {
                let idx = val.addr / ctx.target().page_sz();
                let sz = val.vma_size / ctx.target().page_sz();
                ctx.vma_allocator.note_alloc(idx, sz);
            } else if let Some(val) = val.as_data() {
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
            } else if let Some(val) = val.as_res() {
                if let Some(id) = val.res_val_id() {
                    if ctx.next_res_id <= id {
                        ctx.next_res_id = id + 1;
                    }
                }
            }
        })
    }
}

/// Restore generated resources to `ctx`.  
fn restore_res_ctx(ctx: &mut Context, to: usize) {
    let calls_backup = std::mem::take(&mut ctx.calls);
    for call in &calls_backup[0..to] {
        foreach_call_arg(call, |val| {
            if let Some(val) = val.as_res() {
                let ty = val.ty(ctx.target()).checked_as_res();
                let kind = ty.res_name();
                if val.dir() != Dir::In && val.is_res() {
                    ctx.record_res(kind, val.res_val_id().unwrap());
                }
            }
        })
    }
    ctx.calls = calls_backup;
}

/// Mapping resource id of `calls`, make sure all `res_id` in `calls` is bigger then current `next_res_id`
fn mapping_res_id(ctx: &mut Context, calls: &mut [Call]) {
    for call in calls {
        foreach_call_arg_mut(call, |val| {
            if let Some(val) = val.as_res_mut() {
                match &mut val.kind {
                    ResValueKind::Ref(id) | ResValueKind::Own(id) => *id += ctx.next_res_id,
                    ResValueKind::Null => (),
                }
            }
        })
    }
}

/// Fixup the addr of ptr value.
pub fn fixup(ctx: &mut Context) {
    let mut calls_backup = std::mem::take(&mut ctx.calls);

    // fixup ptr address
    for call in &mut calls_backup {
        foreach_call_arg_mut(call, |val| {
            if let Some(val) = val.as_ptr_mut() {
                if let Some(pointee) = val.pointee.as_mut() {
                    let addr = ctx.mem_allocator.alloc(pointee.layout(ctx.target()));
                    val.addr = addr;
                }
            }
        })
    }

    // remap res id
    let mut cnt = 0;
    let mut res_map: HashMap<ResValueId, ResValueId> = HashMap::default();
    for call in &mut calls_backup {
        foreach_call_arg_mut(call, |val| {
            if let Some(val) = val.as_res_mut() {
                match &mut val.kind {
                    ResValueKind::Own(id) | ResValueKind::Ref(id) => {
                        let new_id = if let Some(new_id) = res_map.get(id) {
                            *new_id
                        } else {
                            let new_id = cnt;
                            cnt += 1;
                            res_map.insert(*id, new_id);
                            new_id
                        };
                        *id = new_id;
                    }
                    _ => (),
                }
            }
        });
        for (_, ids) in &mut call.generated_res {
            for id in ids {
                *id = res_map[id];
            }
        }
        for (_, ids) in &mut call.used_res {
            for id in ids {
                *id = res_map[id];
            }
        }
    }

    // calculate length args
    for call in &mut calls_backup {
        calculate_len_call(ctx.target, call)
    }

    ctx.calls = calls_backup;
}

fn foreach_call_arg(call: &Call, mut f: impl FnMut(&Value)) {
    for arg in call.args() {
        foreach_value(arg, &mut f)
    }
    if let Some(ret) = call.ret.as_ref() {
        foreach_value(ret, &mut f);
    }
}

fn foreach_value(val: &Value, f: &mut dyn FnMut(&Value)) {
    use ValueKind::*;

    match val.kind() {
        Integer | Vma | Data | Res => f(val),
        Ptr => {
            f(val);
            let val = val.checked_as_ptr();
            if let Some(pointee) = &val.pointee {
                foreach_value(pointee, f);
            }
        }
        Group => {
            f(val);
            let val = val.checked_as_group();
            for v in &val.inner {
                foreach_value(v, f);
            }
        }
        ValueKind::Union => {
            f(val);
            let val = val.checked_as_union();
            foreach_value(&val.option, f);
        }
    }
}

fn foreach_call_arg_mut(call: &mut Call, mut f: impl FnMut(&mut Value)) {
    for arg in call.args_mut() {
        foreach_value_mut(arg, &mut f)
    }
    if let Some(ret) = call.ret.as_mut() {
        foreach_value_mut(ret, &mut f);
    }
}

fn foreach_value_mut(val: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    use ValueKind::*;

    match val.kind() {
        Integer | Vma | Data | Res => f(val),
        Ptr => {
            f(val);
            let val = val.checked_as_ptr_mut();
            if let Some(pointee) = val.pointee.as_mut() {
                foreach_value_mut(pointee, f);
            }
        }
        Group => {
            f(val);
            let val = val.checked_as_group_mut();
            for v in &mut val.inner {
                foreach_value_mut(v, f);
            }
        }
        ValueKind::Union => {
            f(val);
            let val = val.checked_as_union_mut();
            foreach_value_mut(&mut val.option, f);
        }
    }
}
