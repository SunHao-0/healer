use crate::target::Target;
use crate::value::VmaValue;
use crate::HashMap;
use crate::{
    context::Context,
    gen::gen_ty_value,
    ty::{Dir, Type, TypeKind},
    value::{PtrValue, Value},
    RngType,
};
use rand::prelude::*;
use std::cell::RefCell;
use std::ops::RangeInclusive;

thread_local! {
    static REC_DEPTH: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
}

#[inline]
fn inc_rec_of(k: &str) {
    REC_DEPTH.with(|rec| {
        let mut rec = rec.borrow_mut();
        if !rec.contains_key(k) {
            rec.insert(k.to_string(), 0);
        }
        *rec.get_mut(k).unwrap() += 1;
    })
}

#[inline]
fn dec_rec_of(k: &str) {
    REC_DEPTH.with(|rec| {
        let mut rec = rec.borrow_mut();
        if let Some(v) = rec.get_mut(k) {
            *v -= 1;
            if *v == 0 {
                rec.remove(k);
            }
        }
    })
}

#[inline]
fn rec_depth_of(k: &str) -> usize {
    REC_DEPTH.with(|rec| {
        let rec = rec.borrow();
        rec.get(k).copied().unwrap_or(0)
    })
}

pub fn gen_ptr(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    use TypeKind::*;

    let ptr_ty = ty.checked_as_ptr();
    let target = ctx.target();
    let elem_ty = target.ty_of(ptr_ty.elem());
    let mut rec: Option<&str> = None;

    if ptr_ty.optional() && matches!(elem_ty.kind(), Struct | Union | Array) {
        if rec_depth_of(elem_ty.name()) >= 2 {
            return PtrValue::new_special(ty.id(), dir, 0).into();
        }
        rec = Some(elem_ty.name());
        inc_rec_of(elem_ty.name());
    }

    let val = if !target.special_ptrs().is_empty() && rng.gen_ratio(1, 1000) {
        let index = rng.gen_range(0..target.special_ptrs().len());
        PtrValue::new_special(ptr_ty.id(), dir, index as u64)
    } else {
        let elem_val = gen_ty_value(ctx, rng, elem_ty, ptr_ty.dir());
        let addr = ctx.mem_allocator().alloc(elem_val.layout(target));
        PtrValue::new(ptr_ty.id(), dir, addr, elem_val)
    };
    if let Some(rec) = rec {
        dec_rec_of(rec);
    }
    val.into()
}

pub fn gen_vma(ctx: &mut Context, rng: &mut RngType, ty: &Type, dir: Dir) -> Value {
    let ty = ty.checked_as_vma();
    let page_range = ty
        .range()
        .unwrap_or_else(|| rand_page_range(ctx.target(), rng));
    let page_num = rng.gen_range(page_range);
    let page = ctx.vma_allocator().alloc(rng, page_num);
    VmaValue::new(ty.id(), dir, page * ctx.target().page_sz(), page_num).into()
}

fn rand_page_range(target: &Target, rng: &mut RngType) -> RangeInclusive<u64> {
    match rng.gen_range(0..100) {
        0..=89 => 1..=4,
        90..=98 => 1..=4,
        _ => 1..=(target.page_num() / 4 * 3),
    }
}
