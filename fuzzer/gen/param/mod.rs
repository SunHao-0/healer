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
        Ptr { dir, elem, .. } => todo!(),
        Vma { begin, end } => todo!(),
        Array { range, elem } => todo!(),
        Struct { fields, .. } => todo!(),
        Union { fields } => todo!(),
    }
}

/// Calculate length type of a call.
pub(super) fn calculate_length_params(call: &mut Call) {
    todo!()
}

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

    if dir == Dir::Out || dir == Dir::InOut {
        let res = Rc::new(ResValue::new_res(0, ctx.next_id()));
        ctx.add_res(Rc::clone(&ty), Rc::clone(&res));
        Value::new_res(dir, ty, res)
    } else {
        // reuse
        if let Some(generated_res) = ctx.generated_res.get(&ty) {
            if !generated_res.is_empty() {
                let res = Rc::clone(generated_res.iter().choose(&mut thread_rng()).unwrap());
                Value::new_res_ref(dir, ty, res)
            } else {
                let val = special_value();
                Value::new_res_null(dir, ty, val)
            }
        } else {
            let val = special_value();
            Value::new_res_null(dir, ty, val)
        }
    }
}
