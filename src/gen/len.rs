use crate::gen::context::GenContext;
use crate::model::{LenInfo, TypeKind, Value, ValueKind};
use rustc_hash::FxHashMap;

/// Finish the calculation of 'len' type.
/// Previous calculation may failed due to ungenerated syscall parameter, so we need calculate them here.
pub(super) fn finish_cal(ctx: &mut GenContext) {
    let left_len_ty = ctx.call_ctx.left_len_vals.split_off(0);
    for (scalar_val_ref, len_info) in left_len_ty {
        cal_syscall_param_len(ctx, unsafe { scalar_val_ref.as_mut().unwrap() }, &len_info)
    }
}

/// Try to calculate value of len type of 'val'
/// Insert ptr of value storage and leninfo to ctx if the calculation failed.
pub(super) fn try_cal(ctx: &mut GenContext, val: &mut Value) {
    let val = val.inner_val_mut().unwrap(); // Can't be null ptr.
    let mut parent_map: Option<FxHashMap<*const Value, &Value>> = None;

    match &mut val.kind {
        ValueKind::Scalar(scalar_val_ref) => {
            if let Some(len_info) = val.ty.len_info() {
                try_cal_syscall_param_len(ctx, scalar_val_ref, len_info)
            }
        }
        ValueKind::Group { .. } | ValueKind::Union { .. } => iter_struct_val(val, |v| {
            if parent_map.is_none() {
                parent_map = Some(build_parent_map(val));
            }
            handle_struct(ctx, v, parent_map.as_ref().unwrap())
        }),
        _ => unreachable!(),
    }
}

fn handle_struct(ctx: &mut GenContext, val: &Value, parent_map: &FxHashMap<*const Value, &Value>) {
    let vals = val.group_val().unwrap();
    for v in vals {
        let v = if let Some(v) = v.inner_val() {
            v
        } else {
            continue;
        };
        if let ValueKind::Scalar(scalar_val_ref) = &v.kind {
            if let Some(len_info) = v.ty.len_info() {
                #[allow(mutable_transmutes, clippy::transmute_ptr_to_ptr)]
                let scalar_val_ref: &mut u64 = unsafe { std::mem::transmute(scalar_val_ref) };
                if &*len_info.path[0] == "syscall" {
                    try_cal_syscall_param_len(ctx, scalar_val_ref, len_info);
                } else {
                    *scalar_val_ref =
                        cal_struct_field_len(val, &len_info.path, &*len_info, Some(parent_map));
                }
            }
        }
    }
}

fn cal_syscall_param_len(ctx: &mut GenContext, scalar_val_ref: &mut u64, len_info: &LenInfo) {
    *scalar_val_ref = try_cal_syscall_param_len_inner(ctx, &*len_info)
        .unwrap_or_else(|| panic!("Failed to calculate length of system param: {:?}", len_info));
}

fn try_cal_syscall_param_len(ctx: &mut GenContext, scalar_val_ref: &mut u64, len_info: &LenInfo) {
    if let Some(val) = try_cal_syscall_param_len_inner(ctx, len_info) {
        *scalar_val_ref = val;
    } else {
        ctx.record_len_to_call_ctx((scalar_val_ref as *mut u64, len_info.clone()))
    }
}

fn try_cal_syscall_param_len_inner(ctx: &mut GenContext, len_info: &LenInfo) -> Option<u64> {
    assert!(len_info.path.len() >= 1);
    assert!(ctx.generating_syscall().is_some());

    let generating_syscall = ctx.generating_syscall().unwrap(); // We're generating a syscall.
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
            return Some(cal_struct_field_len(val, &path[1..], len_info, None));
        }
        return Some(do_cal(generated_params_val, i, len_info));
    }
    None
}

fn do_cal<T: std::borrow::Borrow<Value>>(parent: &[T], target: usize, len_info: &LenInfo) -> u64 {
    let bz = if len_info.bit_sz == 0 {
        8
    } else {
        len_info.bit_sz
    };

    if len_info.offset {
        parent[0..target]
            .iter()
            .map(|f| f.borrow().size())
            .sum::<u64>()
            * 8
            / bz
    } else {
        if target == parent.len() {
            return parent.iter().map(|f| f.borrow().size()).sum::<u64>() * 8 / bz;
        };
        let v = if let Some(v) = parent[target].borrow().inner_val() {
            v
        } else {
            return 0;
        };

        match &v.ty.kind {
            TypeKind::Vma { .. } => {
                let vma_size = v.vma_size().unwrap();
                vma_size * 8 / bz
            }
            TypeKind::Array { .. } => {
                if len_info.bit_sz != 0 {
                    v.size() * 8 / bz
                } else {
                    v.group_val().unwrap().len() as u64
                }
            }
            _ => v.size() * 8 / bz,
        }
    }
}

fn cal_struct_field_len(
    val: &Value,
    mut path: &[Box<str>],
    len_info: &LenInfo,
    parent_map: Option<&FxHashMap<*const Value, &Value>>,
) -> u64 {
    let val = val.inner_val().unwrap();
    if &*path[0] == "parent" {
        path = &path[1..]; // we're already in parent struct.
        if path.is_empty() {
            let vals = val.group_val().unwrap();
            return do_cal(vals, vals.len(), len_info);
        }
    }

    if let Some((vals, pos)) = try_locate(val, path) {
        if let Some(vals) = vals {
            do_cal(vals, pos, len_info)
        } else {
            0
        }
    } else {
        let parent_map = parent_map.expect("Fail to calculate length of struct field");
        let root_struct = position(val, parent_map, path);
        if path.len() > 1 {
            cal_struct_field_len(root_struct, &path[1..], len_info, None)
        } else {
            let vals = root_struct.group_val().unwrap();
            do_cal(vals, vals.len(), len_info)
        }
    }
}

fn position<'a>(
    val: &'a Value,
    parent_map: &'a FxHashMap<*const Value, &Value>,
    path: &[Box<str>],
) -> &'a Value {
    if val.ty.template_name() == &*path[0] {
        val
    } else {
        let parent = parent_map.get(&(val as *const Value));
        if parent.is_none() {
            use std::fmt::Write;
            let mut map_str = String::new();
            for (k, v) in parent_map.iter() {
                writeln!(map_str, "\t{} -> {}", unsafe { &(**k).ty.name }, v.ty.name).unwrap();
            }
            panic!(
                "Failed to trace back to calculate length, path: {:?}, val type: {:?}.\nMap:\n{}",
                path, val.ty.name, map_str
            );
        }
        position(parent.unwrap(), parent_map, path)
    }
}

fn try_locate<'a>(val: &'a Value, path: &[Box<str>]) -> Option<(Option<&'a [Value]>, usize)> {
    let vals = val.group_val().unwrap();
    let fields = val.ty.fields().unwrap();
    let elem = &*path[0];

    for (i, val) in vals.iter().enumerate() {
        if elem != &*fields[i].name {
            continue;
        }
        let val = if let Some(v) = val.inner_val() {
            v
        } else {
            return Some((None, 0));
        };

        if path.len() > 1 {
            return try_locate(val, &path[1..]);
        }
        return Some((Some(vals), i));
    }
    None
}

fn build_parent_map(val: &Value) -> FxHashMap<*const Value, &Value> {
    let mut parent_map = FxHashMap::default();
    iter_struct_val(val, |v| {
        let vals = v.group_val().unwrap();
        for val in vals {
            let val = if let Some(val) = val.inner_val() {
                val
            } else {
                continue;
            };
        }
    });
    parent_map
}

fn iter_struct_val<'a, F>(val: &'a Value, mut f: F)
where
    F: FnMut(&'a Value),
{
    iter_struct_val_inner(val, &mut f)
}

fn iter_struct_val_inner<'a, F>(val: &'a Value, f: &mut F)
where
    F: FnMut(&'a Value),
{
    match &val.kind {
        ValueKind::Scalar { .. }
        | ValueKind::Vma { .. }
        | ValueKind::Res { .. }
        | ValueKind::Bytes { .. } => {}
        ValueKind::Ptr { pointee, .. } => {
            if let Some(pointee) = pointee {
                iter_struct_val_inner(pointee, f)
            }
        }
        ValueKind::Group(_) => {
            if val.ty.fields().is_some() {
                f(val);
                let vals = val.group_val().unwrap();
                for v in vals {
                    iter_struct_val_inner(v, f)
                }
            }
        }
        ValueKind::Union { val, .. } => iter_struct_val_inner(val, f),
    }
}
