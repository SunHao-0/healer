pub(super) mod alloc;
mod buffer;
mod scalar;

use super::*;
use crate::model::{Dir, ResValue, TypeKind, Value};
use std::iter::Iterator;

#[derive(Default)]
pub(super) struct GenParamContext {
    /// Counter of len type of current parameter type.
    pub(super) len_type_count: u32,
}

pub(super) fn gen(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Box<Value> {
    ctx.param_ctx.len_type_count = 0; // clear count first;
    let mut val = Box::new(gen_inner(ctx, ty, dir)); // make sure address of value won't change during calculating length.
    if ctx.has_len_param_ctx() {
        // Try to calculate length value here.
        super::len::try_cal(ctx, &mut *val);
    }
    val
}

fn gen_inner(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    use crate::model::TypeKind::*;
    match &ty.kind {
        Const { .. } | Int { .. } | Csum { .. } | Len { .. } | Proc { .. } | Flags { .. } => {
            scalar::gen(ctx, ty, dir)
        }
        Buffer { .. } => buffer::gen(ctx, ty, dir),
        Res { .. } => gen_res(ctx, ty, dir),
        Ptr { .. } => gen_ptr(ctx, ty, dir),
        Vma { .. } => gen_vma(ctx, ty, dir),
        Array { .. } => gen_array(ctx, ty, dir),
        Struct { .. } => gen_struct(ctx, ty, dir),
        Union { .. } => gen_union(ctx, ty, dir),
    }
}

fn gen_union(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    let (idx, field) = ty
        .fields()
        .unwrap()
        .iter()
        .enumerate()
        .choose(&mut thread_rng())
        .unwrap();
    let field_val = gen_inner(ctx, field.ty, field.dir.unwrap_or(dir));
    Value::new(dir, ty, ValueKind::new_union(idx, field_val))
}

fn gen_struct(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    let fields = ty.fields().unwrap();
    let mut vals = Vec::new();
    for field in fields.iter() {
        let dir = field.dir.unwrap_or(dir);
        vals.push(gen_inner(ctx, field.ty, dir));
    }
    Value::new(dir, ty, ValueKind::new_group(vals))
}

fn gen_array(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    let (elem_ty, range) = ty.array_info().unwrap();
    let len = rand_array_len(range);
    let vals = (0..len).map(|_| gen_inner(ctx, elem_ty, dir)).collect();
    Value::new(dir, ty, ValueKind::new_group(vals))
}

fn rand_array_len(range: Option<(u64, u64)>) -> u64 {
    let mut rng = thread_rng();
    if let Some((mut min, mut max)) = range {
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        if min == max {
            max += 1;
        }
        rng.gen_range(min..max)
    } else {
        let mut rng = thread_rng();
        let (min, max) = (rng.gen_range(2..8), rng.gen_range(8..16));
        rng.gen_range(min..max)
    }
}

fn gen_vma(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    let page_num = rand_vma_num(ctx);
    Value::new(
        dir,
        ty,
        ValueKind::new_vma(
            ctx.vma_alloc.alloc(page_num) * ctx.target.page_sz,
            page_num * ctx.target.page_sz,
        ),
    )
}

fn rand_vma_num(ctx: &GenContext) -> u64 {
    let mut rng = thread_rng();
    if rng.gen::<f32>() < 0.85 {
        rng.gen_range(1..9)
    } else {
        rng.gen_range(1..ctx.target.page_num as u64 / 4)
    }
}

/// Generate value for ptr type.
fn gen_ptr(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    let (elem_ty, elem_dir) = ty.ptr_info().unwrap();

    // Handle recusive type or circle reference here.
    if ty.optional
        && matches!(elem_ty.kind, TypeKind::Struct{..} | TypeKind::Union{..} | TypeKind::Array{..})
    {
        let depth = ctx.inc_rec_depth(elem_ty);
        if depth >= 3 {
            ctx.dec_rec_depth(elem_ty);
            return Value::new(dir, ty, ValueKind::new_ptr_null());
        }
    }

    let elem_val = gen_inner(ctx, elem_ty, elem_dir);
    let addr = ctx.mem_alloc.alloc(elem_val.size(), elem_ty.align);
    // TODO use a recusive depth guard here.
    if ty.optional
        && matches!(elem_ty.kind, TypeKind::Struct{..} | TypeKind::Union{..} | TypeKind::Array{..})
    {
        ctx.dec_rec_depth(elem_ty);
    }
    Value::new(dir, ty, ValueKind::new_ptr(addr, Some(elem_val)))
}

/// Generate value for resource type.
fn gen_res(ctx: &mut GenContext, ty: TypeRef, dir: Dir) -> Value {
    let special_value = || {
        let mut rng = thread_rng();
        ty.res_desc()
            .unwrap()
            .vals
            .iter()
            .copied()
            .choose(&mut rng)
            .unwrap_or_else(|| rng.gen())
    };
    let mut rng = thread_rng();
    if dir == Dir::Out || dir == Dir::InOut {
        let res = Arc::new(ResValue::new_res(0, ctx.next_id()));
        ctx.add_res(ty, Arc::clone(&res));
        Value::new(dir, ty, ValueKind::new_res(res))
    } else {
        // For most case, we reuse the generated resource even if the resource may not be the
        // precise one.
        if !ctx.generated_res.is_empty() && rng.gen::<f32>() < 0.998 {
            // If we've already generated required resource, just reuse it.
            if let Some(generated_res) = ctx.generated_res.get(&ty) {
                if !generated_res.is_empty() {
                    let res = Arc::clone(generated_res.iter().choose(&mut rng).unwrap());
                    return Value::new(dir, ty, ValueKind::new_res_ref(res));
                }
            }
            // Otherwise, try to find the eq resource. Also handle unreachable resource here.
            if let Some(eq_res) = ctx.target.res_eq_class.get(&ty) {
                let mut res_vals = Vec::new();

                for res in eq_res.iter() {
                    if let Some(r) = ctx.generated_res.get(res) {
                        if !r.is_empty() {
                            res_vals.extend(r.iter());
                        }
                    }
                }
                if !res_vals.is_empty() {
                    let res = Arc::clone(res_vals.into_iter().choose(&mut rng).unwrap());
                    return Value::new(dir, ty, ValueKind::new_res_ref(res));
                }
            }
            // We still haven't found any usable resource, try to choose a arbitrary generated
            // resource. May be we can use resource centric strategy here just like syzkaller.
            if let Some((_, vals)) = ctx.generated_res.iter().choose(&mut rng) {
                if !vals.is_empty() && rng.gen::<f32>() < 0.9 {
                    let res = Arc::clone(vals.iter().choose(&mut rng).unwrap());
                    return Value::new(dir, ty, ValueKind::new_res_ref(res));
                }
            }
            // This is bad, use special value.
            let val = special_value();
            Value::new(dir, ty, ValueKind::new_res_null(val))
        } else {
            let val = special_value();
            Value::new(dir, ty, ValueKind::new_res_null(val))
        }
    }
}
