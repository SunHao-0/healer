mod scalar;
// mod buffer;

use super::*;
use hlang::ast::{Dir, Type, TypeKind, Value};
use std::iter::Iterator;
use std::rc::Rc;

pub(super) fn gen(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    todo!()
}

/// Calculate length type of a call.
pub(super) fn calculate_length_params(call: &mut Call) {
    todo!()
}

fn gen_type(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    use hlang::ast::TypeKind::*;
    match &ty.kind {
        Const { .. } | Int { .. } | Csum { .. } | Len { .. } | Proc { .. } | Flags { .. } => {
            scalar::gen(ctx, ty, dir)
        }
        Buffer { .. } => todo!(), //buffer::gen(ctx, ty, dir),
        Res { .. } => gen_res(ctx, ty, dir),
        Ptr { dir, elem, .. } => todo!(),
        Vma { begin, end } => todo!(),
        Array { range, elem } => todo!(),
        Struct { fields, .. } => todo!(),
        Union { fields } => todo!(),
    }
}

fn gen_res(ctx: &mut GenContext, ty: Rc<Type>, dir: Dir) -> Value {
    todo!()
}
