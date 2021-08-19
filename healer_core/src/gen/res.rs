//! Generate value for `resource` type.
use crate::{
    context::Context,
    gen::{current_builder, gen_one_call},
    target::Target,
    ty::{Dir, ResKind, ResType, Type},
    value::{ResValue, ResValueId, Value},
    RngType,
};
use rand::{prelude::SliceRandom, Rng};
use std::cell::Cell;

type ResGenerator = fn(&mut Context, &mut RngType, &ResType, Dir) -> Option<Value>;
const RES_GENERATORS: [ResGenerator; 2] = [res_reusing, generate_res_output_call];

pub fn gen_res(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_res();

    if dir == Dir::Out {
        return generate_res(ctx, ty);
    }

    for g in RES_GENERATORS {
        if rng.gen_ratio(1, 10) {
            continue;
        }
        if let Some(val) = g(ctx, rng, ty, dir) {
            return val;
        }
    }

    let val = ty.special_vals().choose(rng).copied().unwrap_or(0);
    ResValue::new_null(ty.id(), dir, val).into()
}

#[inline]
fn generate_res(ctx: &mut Context, ty: &ResType) -> Value {
    let id = ctx.next_res_id();
    current_builder(|c| {
        c.record_res(ty.res_name(), id);
    });
    ctx.record_res(ty.res_name(), id);
    ResValue::new_res(ty.id(), id, ty.special_vals()[0]).into()
}

#[inline]
fn record_used_res(kind: &ResKind, id: ResValueId) {
    current_builder(|c| {
        c.used_res(kind, id);
    });
}

thread_local! {
    static GENERATING_RES: Cell<bool> = Cell::new(false);
}

#[inline]
fn mark_generating() {
    GENERATING_RES.with(|g| g.set(true))
}

#[inline]
fn generating() -> bool {
    GENERATING_RES.with(|g| g.get())
}

#[inline]
fn generate_done() {
    GENERATING_RES.with(|g| g.set(false))
}

fn generate_res_output_call(
    ctx: &mut Context,
    rng: &mut RngType,
    ty: &ResType,
    dir: Dir,
) -> Option<Value> {
    if generating() {
        return None;
    }

    mark_generating();
    verbose!("generating resource: {}", ty.res_name());
    let mut ret = None;
    let target = ctx.target();
    let mut tries = 0;
    while tries != 3 {
        let kind = res_mapping(target, rng, ty.res_name());
        let candidates = target.res_output_syscall(kind);
        if candidates.is_empty() {
            tries += 1;
            continue;
        }
        let sid = candidates.choose(rng).unwrap();
        gen_one_call(ctx, rng, *sid);
        let new_res = &ctx.calls().last().unwrap().generated_res;
        if let Some(ids) = new_res.get(kind) {
            if let Some(id) = ids.choose(rng) {
                record_used_res(kind, *id);
                ret = Some(ResValue::new_ref(ty.id(), dir, *id).into());
                break;
            }
        }
        tries += 1;
    }
    generate_done();

    ret
}

/// Mapping resource to its super/sub type randomly
fn res_mapping<'a>(target: &'a Target, rng: &mut RngType, kind: &'a ResKind) -> &'a ResKind {
    if target.res_output_syscall(kind).is_empty() || rng.gen_ratio(1, 10) {
        do_res_mapping(target, rng, kind)
    } else {
        kind
    }
}

fn do_res_mapping<'a>(target: &'a Target, rng: &mut RngType, kind: &'a ResKind) -> &'a ResKind {
    let kinds = if !target.res_sub_tys(kind).is_empty() && rng.gen_ratio(4, 5) {
        target.res_sub_tys(kind)
    } else {
        target.res_super_tys(kind)
    };

    let mut tries = 0;
    let max = std::cmp::min(kinds.len(), 16);
    let mut ret = kind;
    while tries != max {
        let kind = kinds.choose(rng).unwrap();
        if !target.res_output_syscall(kind).is_empty() {
            ret = kind;
            break;
        }
        tries += 1;
    }
    ret
}

fn res_reusing(ctx: &mut Context, rng: &mut RngType, ty: &ResType, dir: Dir) -> Option<Value> {
    let kinds = ctx
        .res()
        .iter()
        .filter(|r| should_use(ctx.target(), rng, ty, r))
        .collect::<Vec<_>>();
    let kind = kinds.choose(rng).copied()?;
    let id = ctx.res_ids()[kind].choose(rng).copied()?;
    record_used_res(kind, id);
    Some(ResValue::new_ref(ty.id(), dir, id).into())
}

fn should_use(target: &Target, rng: &mut RngType, dst: &ResType, src_kind: &ResKind) -> bool {
    let dst_kind = dst.res_name();
    let is_sub_ty = target.res_sub_tys(dst_kind).binary_search(src_kind).is_ok();
    if is_sub_ty {
        return true;
    }
    let dst_kind = &dst.kinds()[0];
    let use_similar =
        rng.gen_ratio(1, 50) && target.res_sub_tys(dst_kind).binary_search(src_kind).is_ok();
    use_similar
}
