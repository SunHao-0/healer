//! Generate value for `array`, `struct`, `union` type.
use crate::{
    context::Context,
    gen::gen_ty_value,
    ty::{Dir, Type},
    value::{GroupValue, UnionValue, Value},
    RngType,
};
use rand::prelude::*;
use std::ops::RangeInclusive;

pub fn gen_array(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_array();
    let elem_ty = ty.elem();
    let range = ty.range().unwrap_or(RangeInclusive::new(0, 10));
    let len = rng.gen_range(range);
    let elems = (0..len)
        .map(|_| gen_ty_value(ctx, rng, elem_ty, dir))
        .collect::<Vec<_>>();
    GroupValue::new(ty.id(), dir, elems).into()
}

pub fn gen_struct(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_struct();
    let mut fields = Vec::with_capacity(ty.fields().len());
    for field in ty.fields() {
        let dir = field.dir().unwrap_or(dir);
        fields.push(gen_ty_value(ctx, rng, field.ty(), dir));
    }
    GroupValue::new(ty.id(), dir, fields).into()
}

pub fn gen_union(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_union();
    let field_to_gen = rng.gen_range(0..ty.fields().len());
    let field = &ty.fields()[field_to_gen];
    let field_ty = field.ty();
    let field_dir = field.dir().unwrap_or(dir);
    let val = gen_ty_value(ctx, rng, field_ty, field_dir);
    UnionValue::new(ty.id(), dir, field_to_gen as u64, val).into()
}
