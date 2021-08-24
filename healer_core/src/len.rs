use crate::{
    mutation::{foreach_value, foreach_value_mut},
    prog::Call,
    target::Target,
    ty::{Field, LenType, TypeKind},
    value::{IntegerValue, Value},
    HashMap,
};
use std::{ptr::null, slice::from_raw_parts};

/// Calculate length value, same as Syzkaller
#[inline]
pub(crate) fn calculate_len(target: &Target, call: &mut Call) {
    let syscall = target.syscall_of(call.sid());
    calculate_len_args_inner(target, syscall.params(), call.args_mut());
}

#[inline]
pub(crate) fn calculate_len_args(target: &Target, fields: &[Field], args: &mut [Value]) {
    calculate_len_args_inner(target, fields, args)
}

struct LenContext<'a> {
    target: &'a Target,
    parent_map: HashMap<*const Value, *const Value>,
    current_node: *const Value,
    syscall_args: RawSlice<Value>,
    syscall_fields: RawSlice<Field>,
}

#[derive(Clone)]
struct RawSlice<T> {
    ptr: *const T,
    len: usize,
}

impl<T> RawSlice<T> {
    pub fn new(s: &[T]) -> Self {
        Self {
            ptr: s.as_ptr(),
            len: s.len(),
        }
    }

    unsafe fn into_slice(self) -> &'static [T] {
        from_raw_parts(self.ptr, self.len)
    }
}

fn calculate_len_args_inner(target: &Target, fields: &[Field], args: &mut [Value]) {
    let mut parent_map = HashMap::new();
    for arg in args.iter() {
        foreach_value(arg, |val| {
            let ty = val.ty(target);
            if ty.as_struct().is_some() {
                let group = val.checked_as_group();
                for inner in &group.inner {
                    if let Some(inner) = inner_val(inner) {
                        parent_map.insert(inner as *const Value, val as *const Value);
                    }
                }
            }
        })
    }

    let mut ctx = LenContext {
        target,
        parent_map,
        current_node: null(),
        syscall_args: RawSlice::new(args),
        syscall_fields: RawSlice::new(fields),
    };

    find_cal_len_arg(&mut ctx, args, fields);

    for arg in args {
        foreach_value_mut(arg, |val| {
            let ty = val.ty(target);
            if let Some(struct_ty) = ty.as_struct() {
                let group_val = val.checked_as_group_mut();
                find_cal_len_arg(&mut ctx, &mut group_val.inner, struct_ty.fields())
            }
        })
    }
}

fn inner_val(val: &Value) -> Option<&Value> {
    if let Some(ptr_val) = val.as_ptr() {
        if let Some(pointee) = ptr_val.pointee.as_ref() {
            inner_val(pointee)
        } else {
            None
        }
    } else {
        Some(val)
    }
}

fn inner_val_mut(val: &mut Value) -> Option<&'static mut Value> {
    if let Some(ptr_val) = val.as_ptr_mut() {
        if let Some(pointee) = ptr_val.pointee.as_mut() {
            inner_val_mut(pointee)
        } else {
            None
        }
    } else {
        Some(unsafe { std::mem::transmute(val) })
    }
}

const SYSCALL_REF: &str = "syscall";
const PARENT_REF: &str = "parent";

/// Find and calculate all length args.
fn find_cal_len_arg(ctx: &mut LenContext, args: &mut [Value], fields: &[Field]) {
    let args_raw_slice = RawSlice::new(args);
    for val in args.iter_mut().filter_map(inner_val_mut) {
        let current_node = val as *const _;
        let ty = val.ty(ctx.target);
        if ty.as_len().is_none() {
            continue;
        }
        let path = ty.checked_as_len().path().to_vec();
        let dst = val.checked_as_int_mut();

        if &path[0][..] == SYSCALL_REF {
            do_calculation(
                ctx,
                dst,
                &path[1..],
                ctx.syscall_args.clone(),
                ctx.syscall_fields.clone(),
            );
        } else {
            ctx.current_node = current_node;
            do_calculation(
                ctx,
                dst,
                &path,
                args_raw_slice.clone(),
                RawSlice::new(fields),
            );
        }
    }
}

fn struct_inner(target: &Target, val: &Value) -> Option<(RawSlice<Value>, RawSlice<Field>)> {
    let ty = val.ty(target);
    if let Some(struct_ty) = ty.as_struct() {
        let val = val.checked_as_group();
        Some((
            RawSlice::new(&val.inner[..]),
            RawSlice::new(struct_ty.fields()),
        ))
    } else {
        None
    }
}

fn do_calculation(
    ctx: &mut LenContext,
    dst: &mut IntegerValue,
    mut path: &[Box<str>],
    vals: RawSlice<Value>,
    fields: RawSlice<Field>,
) {
    let elem = &path[0][..];
    path = &path[1..];
    let mut offset = 0;
    let vals = unsafe { vals.into_slice() };
    let fields = unsafe { fields.into_slice() };

    for (val, field) in vals.iter().zip(fields.iter()) {
        if elem != field.name() {
            offset += val.size(ctx.target);
            continue;
        }
        if let Some(val) = inner_val(val) {
            if !path.is_empty() {
                if let Some((vals, fields)) = struct_inner(ctx.target, val) {
                    do_calculation(ctx, dst, path, vals, fields);
                } else {
                    dst.val = 0;
                }
            } else {
                dst.val =
                    do_calculate_len(ctx.target, val, offset, dst.ty(ctx.target).checked_as_len());
            }
        } else {
            dst.val = 0;
        }
        return;
    }

    if elem == PARENT_REF && !ctx.current_node.is_null() {
        let parent = ctx.parent_map[&ctx.current_node];
        ctx.current_node = parent;
        let val = unsafe { parent.as_ref().unwrap() };
        if !path.is_empty() {
            if let Some((vals, fields)) = struct_inner(ctx.target, val) {
                do_calculation(ctx, dst, path, vals, fields);
            } else {
                dst.val = 0;
            }
        } else {
            dst.val =
                do_calculate_len(ctx.target, val, offset, dst.ty(ctx.target).checked_as_len());
        }
        return;
    }

    while !ctx.current_node.is_null() {
        let parent = ctx.parent_map.get(&ctx.current_node).cloned();
        ctx.current_node = parent.unwrap_or(null());
        if !ctx.current_node.is_null() {
            let val = unsafe { ctx.current_node.as_ref().unwrap() };
            let ty = val.ty(ctx.target);
            if ty.template_name() != elem {
                continue;
            }

            if !path.is_empty() {
                if let Some((vals, fields)) = struct_inner(ctx.target, val) {
                    do_calculation(ctx, dst, path, vals, fields);
                } else {
                    dst.val = 0;
                }
            } else {
                dst.val =
                    do_calculate_len(ctx.target, val, offset, dst.ty(ctx.target).checked_as_len());
            }
            return;
        }
    }
    dst.val = 0; // tolerate errors from discription.
}

fn do_calculate_len(target: &Target, val: &Value, offset: u64, len_ty: &LenType) -> u64 {
    use TypeKind::*;
    let mut bit_sz = len_ty.len_bit_size();
    if bit_sz == 0 {
        bit_sz = 8;
    }
    if len_ty.offset() {
        return if offset != 0 {
            (offset * 8) / bit_sz
        } else {
            0
        };
    }

    let ty = val.ty(target);
    let mut ret = 0;
    match ty.kind() {
        Vma => {
            let vma_val = val.checked_as_vma();
            if vma_val.vma_size != 0 {
                ret = vma_val.vma_size * 8 / bit_sz
            }
        }
        Array => {
            let group_val = val.checked_as_group();
            if len_ty.bit_size() != 0 {
                let sz_byte = group_val.size(target);
                if sz_byte != 0 {
                    ret = (sz_byte * 8) / bit_sz;
                }
            } else {
                ret = group_val.inner.len() as u64;
            }
        }
        _ => {
            let sz_byte = val.size(target);
            if sz_byte != 0 {
                ret = (sz_byte * 8) / bit_sz;
            }
        }
    }

    ret
}
