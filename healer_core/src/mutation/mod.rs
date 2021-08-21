//! Prog mutation.
use crate::{
    alloc::Allocator,
    context::Context,
    corpus::CorpusWrapper,
    gen::{choose_weighted, prog_len_range},
    mutation::{
        call::mutate_call_args,
        seq::{insert_calls, remove_call, splice},
    },
    prog::{Call, Prog},
    relation::Relation,
    target::Target,
    ty::{Dir, ResKind, TypeKind},
    value::{ResValueId, ResValueKind, Value, ValueKind},
    HashMap, HashSet, RngType,
};
use rand::prelude::*;

pub mod buffer;
pub mod call;
pub mod group;
pub mod int;
pub mod ptr;
pub mod res;
pub mod seq;

/// Muate input prog `p`
pub fn mutate(
    target: &Target,
    relation: &Relation,
    corpus: &CorpusWrapper,
    rng: &mut RngType,
    p: &mut Prog,
) -> bool {
    type MutateOperation = fn(&mut Context, &CorpusWrapper, &mut RngType) -> bool;
    const OPERATIONS: [MutateOperation; 4] = [insert_calls, mutate_call_args, splice, remove_call];
    const WEIGHTS: [u64; 4] = [400, 980, 999, 1000];

    let calls = std::mem::take(&mut p.calls);
    let mut ctx = Context::new(target, relation);
    restore_partial_ctx(&mut ctx, &calls);
    ctx.calls = calls;

    let mut mutated = false;
    let mut tries = 0;
    while tries < 128 && (!mutated || ctx.calls.is_empty() || rng.gen_ratio(1, 2)) {
        let idx = choose_weighted(rng, &WEIGHTS);
        debug_info!("using strategy-{}", idx);
        mutated = OPERATIONS[idx](&mut ctx, corpus, rng);

        if ctx.calls.len() >= prog_len_range().end {
            remove_extra_calls(&mut ctx);
        }

        // clear mem usage, fixup later
        ctx.mem_allocator.restore();
        // clear newly added res
        ctx.res_ids.clear();
        ctx.res_kinds.clear();

        tries += 1;
    }

    if mutated {
        fixup(ctx.target, &mut ctx.calls);
    }
    *p = ctx.to_prog();

    mutated
}

#[inline]
fn remove_extra_calls(ctx: &mut Context) {
    let r = ctx.calls.len() - prog_len_range().end + 1;
    debug_info!("remove_extra_calls: remove {} calls", r);
    let mut calls = std::mem::take(&mut ctx.calls);
    calls.drain(calls.len() - r..);
    ctx.calls = calls;
}

/// Fixup the addr of ptr value, re-order res id and re-collect generated res and used res of calls.
pub fn fixup(target: &Target, calls: &mut [Call]) {
    let mut cnt = 0;
    let mut res_map: HashMap<ResValueId, ResValueId> = HashMap::default();
    let mut allocator = Allocator::new(target.page_sz() * target.page_num());

    for call in calls {
        let mut grs: HashMap<ResKind, HashSet<ResValueId>> = HashMap::new();
        let mut urs: HashMap<ResKind, HashSet<ResValueId>> = HashMap::new();

        foreach_call_arg_mut(call, |val| {
            use ResValueKind::*;

            // fixup ptr address
            if let Some(val) = val.as_ptr_mut() {
                if let Some(pointee) = val.pointee.as_mut() {
                    let addr = allocator.alloc(pointee.layout(target));
                    val.addr = addr;
                }
            }

            if val.kind() != ValueKind::Res {
                return;
            }

            // remap res id
            let val = val.checked_as_res_mut();
            match &mut val.kind {
                Own(id) => {
                    let entry = res_map.entry(*id).or_insert_with(|| {
                        let new_id = cnt;
                        cnt += 1;
                        new_id
                    });
                    *id = *entry;
                }
                Ref(id) => {
                    if let Some(new_id) = res_map.get(id) {
                        *id = *new_id;
                    } else {
                        debug_warn!("fixup: res-{} missing", id);
                    }
                }
                _ => (),
            }

            let ty = val.ty(target).checked_as_res();
            if let Some(id) = val.res_val_id() {
                if val.own_res() {
                    if !grs.contains_key(ty.res_name()) {
                        grs.insert(ty.res_name().clone(), HashSet::new());
                    }
                    grs.get_mut(ty.res_name()).unwrap().insert(id);
                } else {
                    if !urs.contains_key(ty.res_name()) {
                        urs.insert(ty.res_name().clone(), HashSet::new());
                    }
                    urs.get_mut(ty.res_name()).unwrap().insert(id);
                }
            }
        });
        call.generated_res = grs
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
        call.used_res = urs
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect();
    }
}

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
                if val.dir() != Dir::In && val.own_res() {
                    ctx.record_res(kind, val.res_val_id().unwrap());
                }
            }
        })
    }
    ctx.calls = calls_backup;
}

fn foreach_call_arg(call: &Call, mut f: impl FnMut(&Value)) {
    for arg in call.args() {
        foreach_value_inner(arg, &mut f)
    }
    if let Some(ret) = call.ret.as_ref() {
        foreach_value_inner(ret, &mut f);
    }
}

#[inline]
fn foreach_value(val: &Value, mut f: impl FnMut(&Value)) {
    foreach_value_inner(val, &mut f)
}

fn foreach_value_inner(val: &Value, f: &mut dyn FnMut(&Value)) {
    use ValueKind::*;

    match val.kind() {
        Integer | Vma | Data | Res => f(val),
        Ptr => {
            f(val);
            let val = val.checked_as_ptr();
            if let Some(pointee) = &val.pointee {
                foreach_value_inner(pointee, f);
            }
        }
        Group => {
            f(val);
            let val = val.checked_as_group();
            for v in &val.inner {
                foreach_value_inner(v, f);
            }
        }
        ValueKind::Union => {
            f(val);
            let val = val.checked_as_union();
            foreach_value_inner(&val.option, f);
        }
    }
}

fn foreach_call_arg_mut(call: &mut Call, mut f: impl FnMut(&mut Value)) {
    for arg in call.args_mut() {
        foreach_value_mut_inner(arg, &mut f)
    }
    if let Some(ret) = call.ret.as_mut() {
        foreach_value_mut_inner(ret, &mut f);
    }
}

fn foreach_value_mut_inner(val: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    use ValueKind::*;

    match val.kind() {
        Integer | Vma | Data | Res => f(val),
        Ptr => {
            f(val);
            let val = val.checked_as_ptr_mut();
            if let Some(pointee) = val.pointee.as_mut() {
                foreach_value_mut_inner(pointee, f);
            }
        }
        Group => {
            f(val);
            let val = val.checked_as_group_mut();
            for v in &mut val.inner {
                foreach_value_mut_inner(v, f);
            }
        }
        ValueKind::Union => {
            f(val);
            let val = val.checked_as_union_mut();
            foreach_value_mut_inner(&mut val.option, f);
        }
    }
}
