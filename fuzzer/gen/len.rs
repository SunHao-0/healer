use super::*;
use hlang::ast::{Field, Value, ValueKind};

/// Calculate value of 'len' type.
pub(super) fn finish_cal(ctx: &mut GenContext) {
    todo!()
}

/// Try to calculate value of len type of 'val'
pub(super) fn try_cal(ctx: &mut GenContext, val: &mut Value) {
    let val = if let Some(val) = val.inner_val_mut() {
        val
    } else {
        return;
    };

    match &mut val.kind {
        ValueKind::Scalar(ref mut scalar_val) => {
            if let Some(len_info) = val.ty.get_len_info() {
                assert_eq!(&*len_info.path[0], "syscall");
                if !try_cal_syscall_param_len(ctx, scalar_val, &*len_info) {
                    ctx.record_len_to_call_ctx((scalar_val as *mut u64, len_info))
                }
            }
        }
        ValueKind::Group { .. } => {
            if let Some(fields) = val.ty.get_fields() {
                todo!()
            }
        }
        ValueKind::Union {
            idx,
            val: union_val,
        } => {
            let fields = val.ty.get_fields().unwrap();
            todo!()
        }
        _ => unreachable!(),
    }
    todo!()
}

fn cal_syscall_param_len() {
    todo!()
}

fn try_cal_syscall_param_len(ctx: &mut GenContext, val: &mut u64, len_info: &LenInfo) -> bool {
    todo!()
}

fn cal_struct_field_len() {
    todo!()
}

pub(super) fn locate() {
    todo!()
}

fn try_locate_in_params<'a>(
    params: &[Param],
    vals: &'a [Value],
    path: &[&str],
) -> Option<(Option<&'a [Value]>, usize)> {
    try_locate_inner(
        &params.iter().map(|p| &*p.name).collect::<Vec<_>>()[..],
        vals,
        path,
    )
}

pub(super) fn try_locate<'a>(
    fields: &[Field],
    vals: &'a [Value],
    path: &[&str],
) -> Option<(Option<&'a [Value]>, usize)> {
    try_locate_inner(
        &fields.iter().map(|f| &*f.name).collect::<Vec<_>>()[..],
        vals,
        path,
    )
}

pub(super) fn try_locate_inner<'a>(
    fields_name: &[&str],
    vals: &'a [Value],
    path: &[&str],
) -> Option<(Option<&'a [Value]>, usize)> {
    let elem = &*path[0];

    for (i, val) in vals.iter().enumerate() {
        if elem != fields_name[i] {
            continue;
        }
        let val = if let Some(v) = val.inner_val() {
            v
        } else {
            return Some((None, 0));
        };

        if path.len() > 1 {
            return try_locate(
                &val.ty.get_fields().unwrap(),
                val.kind.get_group_val().unwrap(),
                &path[1..],
            );
        }
        return Some((Some(vals), i));
    }
    None
}

fn iter_struct_val_mut<F>(val: &mut Value, mut f: F)
where
    F: FnMut(&mut Value),
{
    match &mut val.kind {
        ValueKind::Scalar { .. }
        | ValueKind::Vma { .. }
        | ValueKind::Res { .. }
        | ValueKind::Bytes { .. } => {}
        ValueKind::Ptr { pointee, .. } => {
            if let Some(pointee) = pointee {
                iter_struct_val_mut(pointee, f)
            }
        }
        ValueKind::Group(_) => f(val),
        ValueKind::Union { val, .. } => iter_struct_val_mut(val, f),
    }
}
