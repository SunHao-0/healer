use super::*;
use hlang::ast::{Field, Value, ValueKind};
use rustc_hash::FxHashMap;

/// Calculate value of 'len' type.
pub(super) fn finish_cal(ctx: &mut GenContext) {
    let left_len_ty = ctx.call_ctx.left_len_vals.drain(0..).collect::<Vec<_>>();
    for (scalar_val_ref, len_info) in left_len_ty {
        cal_syscall_param_len(ctx, unsafe { scalar_val_ref.as_mut().unwrap() }, len_info)
    }
}

/// Try to calculate value of len type of 'val'
pub(super) fn try_cal(ctx: &mut GenContext, val: &mut Value) {
    let val = val.inner_val_mut().unwrap(); // Can't be null ptr.
    let mut _parent_map: Option<FxHashMap<&Value, &Value>> = None;

    match &mut val.kind {
        ValueKind::Scalar(ref mut scalar_val_ref) => {
            if let Some(len_info) = val.ty.get_len_info() {
                try_cal_syscall_param_len(ctx, scalar_val_ref, len_info)
            }
        }
        ValueKind::Group { .. } | ValueKind::Union { .. } => iter_struct_val_mut(val, |v| {
                if let TypeKind::Struct{..} = &v.ty.kind{
                    let vals = v.kind.get_group_val().unwrap();
                    for v in vals.iter().filter_map(|v|v.inner_val()){
                        if let Some(len_info) = v.ty.get_len_info(){
                            todo!() // To be continued.
                        }
                    }       
                    todo!()
                }
        }),
        _ => unreachable!(),
    }
    todo!()
}

pub(super) fn cal_syscall_param_len(
    ctx: &mut GenContext,
    scalar_val_ref: &mut u64,
    len_info: Rc<LenInfo>,
) {
    *scalar_val_ref = try_cal_syscall_param_len_inner(ctx, &*len_info).expect(&format!(
        "Failed to calculate length of system param: {:?}",
        len_info,
    ));
}

fn try_cal_syscall_param_len(
    ctx: &mut GenContext,
    scalar_val_ref: &mut u64,
    len_info: Rc<LenInfo>,
) {
    if let Some(val) = try_cal_syscall_param_len_inner(ctx, &*len_info) {
        *scalar_val_ref = val;
    } else {
        ctx.record_len_to_call_ctx((scalar_val_ref as *mut u64, len_info))
    }
}

fn try_cal_syscall_param_len_inner(ctx: &mut GenContext, len_info: &LenInfo) -> Option<u64> {
    assert!(len_info.path.len() >= 1);
    assert!(ctx.get_generating_syscall().is_some());

    let generating_syscall = ctx.get_generating_syscall().unwrap(); // We're generating a syscall.
    let generated_params_val = &ctx.call_ctx.generated_params;
    if generated_params_val.is_empty() {
        return None;
    }
    let generated_params = &generating_syscall.params[0..generated_params_val.len()];
    let path = if "syscall" == &*len_info.path[0] {
        &len_info.path[1..]
    } else {
        &len_info.path[..]
    };
    let elem = &path[0];

    for (i, val) in generated_params_val.iter().enumerate() {
        let p = &generated_params[i];
        if &p.name != elem {
            continue;
        }
        let val = if let Some(val) = val.inner_val() {
            val
        } else {
            return Some(0);
        };

        if path.len() > 1 {
            let fields = val.ty.get_fields().unwrap();
            let vals = val.kind.get_group_val().unwrap();
            return Some(cal_struct_field_len(fields, vals, &path[1..], len_info));
        }
        return Some(do_cal(generated_params_val, i, len_info));
    }
    None
}

fn do_cal(parent: &[Value], target: usize, len_info: &LenInfo) -> u64 {
    if len_info.offset {
        parent[0..target].iter().map(|f| f.size()).sum::<u64>() * 8 / len_info.bit_sz
    } else {
        let v = if let Some(v) = parent[target].inner_val() {
            v
        } else {
            return 0;
        };
        let bz = if len_info.bit_sz == 0 {
            8
        } else {
            len_info.bit_sz
        };

        match &v.ty.kind {
            TypeKind::Vma { .. } => {
                let vma_size = v.kind.get_vma_size().unwrap();
                vma_size * 8 / bz
            }
            TypeKind::Array { .. } => {
                if len_info.bit_sz != 0 {
                    v.size() * 8 / bz
                } else {
                    v.kind.get_group_val().unwrap().len() as u64
                }
            }
            _ => v.size() * 8 / bz,
        }
    }
}

fn cal_struct_field_len(
    fields: &[Field],
    vals: &[Value],
    path: &[Box<str>],
    len_info: &LenInfo,
) -> u64 {
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

fn build_parent_map(val: &Value) -> FxHashMap<&Value, &Value>{
    todo!()
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
