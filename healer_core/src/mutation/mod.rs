//! Prog mutation.
use crate::{
    context::Context,
    corpus::CorpusWrapper,
    gen::{choose_weighted, prog_len_range},
    mutation::{
        call::mutate_call_args,
        seq::{insert_calls, splice},
    },
    prog::{Call, Prog},
    relation::Relation,
    target::Target,
    ty::{Dir, TypeKind},
    value::{ResValueId, ResValueKind, Value, ValueKind},
    verbose, HashMap, RngType,
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
            use ResValueKind::*;
            if val.kind() != ValueKind::Res {
                return;
            }
            let val = val.checked_as_res_mut();
            if let Own(id) | Ref(id) = &mut val.kind {
                let entry = res_map.entry(*id).or_insert_with(|| {
                    let new_id = cnt;
                    cnt += 1;
                    new_id
                });
                *id = *entry;
            }
        });
        for ids in call.generated_res.values_mut() {
            for id in ids {
                *id = res_map[id];
            }
        }
        for ids in call.used_res.values_mut() {
            for id in ids {
                *id = res_map[id];
            }
        }
    }
    ctx.calls = calls_backup;
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
                if val.dir() != Dir::In && val.is_res() {
                    ctx.record_res(kind, val.res_val_id().unwrap());
                }
            }
        })
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
