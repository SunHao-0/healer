pub(super) mod alloc;
mod buffer;
mod scalar;

use super::*;
use hlang::ast::{Dir, ResValue, Type, TypeKind, Value};
use std::iter::Iterator;
use std::rc::Rc;

pub(super) fn gen(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    use hlang::ast::TypeKind::*;
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
        Union { fields } => gen_union(ctx, ty, dir),
    }
}

/// Calculate length type of a call.
pub(super) fn calculate_length_params(call: &mut Call) {
    todo!()
}

fn gen_union(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    let (idx, field) = ty
        .get_fields()
        .unwrap()
        .iter()
        .enumerate()
        .choose(&mut thread_rng())
        .unwrap();
    let field_val = gen(
        ctx,
        Rc::clone(field.ty.as_ref().unwrap()),
        field.dir.unwrap_or(dir),
    );
    Value::new_union(dir, ty, idx, field_val)
}

fn gen_struct(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    let fields = ty.get_fields().unwrap();
    let mut vals = Vec::new();
    for field in fields.iter() {
        let dir = field.dir.unwrap_or(dir);
        vals.push(gen(ctx, Rc::clone(field.ty.as_ref().unwrap()), dir));
    }
    Value::new_group(dir, ty, vals)
}

fn gen_array(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    let (elem_ty, range) = ty.get_array_info().unwrap();
    let len = rand_array_len(range);
    let vals = (0..len)
        .map(|_| gen(ctx, Rc::clone(elem_ty), dir))
        .collect();
    Value::new_group(dir, ty, vals)
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
        rng.gen_range(min, max)
    } else {
        let mut rng = thread_rng();
        let (min, max) = (rng.gen_range(2, 8), rng.gen_range(8, 16));
        rng.gen_range(min, max)
    }
}

fn gen_vma(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    let page_num = rand_vma_num(ctx);
    Value::new_vma(dir, ty, ctx.vma_alloc.alloc(page_num), page_num)
}

fn rand_vma_num(ctx: &GenContext) -> u64 {
    let mut rng = thread_rng();
    if rng.gen::<f32>() < 0.85 {
        rng.gen_range(1, 9)
    } else {
        rng.gen_range(1, ctx.target.page_num as u64 / 4)
    }
}

/// Generate value for ptr type.
fn gen_ptr(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    // Handle recusive type or circle reference here.

    let (elem_ty, elem_dir) = ty.get_ptr_info().unwrap();
    let elem_val = gen(ctx, Rc::clone(elem_ty), elem_dir);
    let addr = ctx.mem_alloc.alloc(elem_val.size(), elem_ty.align);
    Value::new_ptr(dir, ty, addr, Some(elem_val))
}

/// Generate value for resource type.
fn gen_res(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
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
        let res = Rc::new(ResValue::new_res(0, ctx.next_id()));
        ctx.add_res(Rc::clone(&ty), Rc::clone(&res));
        Value::new_res(dir, ty, res)
    } else {
        // For most case, we reuse the generated resource even if the resource may not be the
        // precise one.
        if !ctx.generated_res.is_empty() && rng.gen::<f32>() < 0.998 {
            // If we've already generated required resource, just reuse it.
            if let Some(generated_res) = ctx.generated_res.get(&ty) {
                if !generated_res.is_empty() {
                    let res = Rc::clone(generated_res.iter().choose(&mut rng).unwrap());
                    return Value::new_res_ref(dir, ty, res);
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
                    let res = Rc::clone(res_vals.into_iter().choose(&mut rng).unwrap());
                    return Value::new_res_ref(dir, ty, res);
                }
            }
            // We still haven't found any usable resource, try to choose a arbitrary generated
            // resource.
            if let Some((_, vals)) = ctx.generated_res.iter().choose(&mut rng) {
                if !vals.is_empty() && rng.gen::<f32>() < 0.9 {
                    let res = Rc::clone(vals.iter().choose(&mut rng).unwrap());
                    return Value::new_res_ref(dir, ty, res);
                }
            }
            // This is bad, use special value.
            let val = special_value();
            Value::new_res_null(dir, ty, val)
        } else {
            let val = special_value();
            Value::new_res_null(dir, ty, val)
        }
    }
}
