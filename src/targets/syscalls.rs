use crate::model::{
    BinFmt, BufferKind, CsumKind, Dir, Field, IntFmt, LenInfo, Param, ResDesc, Syscall,
    SyscallAttr, SyscallRef, TextKind, Type, TypeKind, TypeRef,
};
use crate::targets::sys_json::TARGETS;
use crate::utils::to_boxed_str;
use json::JsonValue;
use rustc_hash::{FxHashMap, FxHashSet};
use std::mem;
use std::sync::Mutex;

lazy_static! {
    #[allow(clippy::type_complexity)]
    static ref DESCS: Mutex<[Option<(&'static [Syscall], &'static [Type])>; 17]> =
        Mutex::new([None; 17]);
}

pub fn parse<T: AsRef<str>>(
    target: T,
    desc_json: &JsonValue,
) -> (Box<[SyscallRef]>, Box<[TypeRef]>) {
    let target = target.as_ref();
    let idx = TARGETS
        .iter()
        .copied()
        .position(|(t, _)| t == target)
        .unwrap();

    let mut desc = DESCS.lock().unwrap();
    if desc[idx].is_none() {
        do_parse(desc_json, &mut desc[idx]);
    }

    let (syscalls, tys) = &desc[idx].unwrap();
    let syscalls = syscalls.iter().collect::<Vec<_>>();
    let tys = tys.iter().map(|ty| TypeRef::Ref(ty)).collect::<Vec<_>>();
    (syscalls.into_boxed_slice(), tys.into_boxed_slice())
}

fn do_parse(desc_json: &JsonValue, out: &mut Option<(&'static [Syscall], &'static [Type])>) {
    let mut calls: Box<[Syscall]> = desc_json["Syscalls"]
        .members()
        .enumerate()
        .map(|(id, call)| parse_syscall(id, call))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let res: FxHashMap<Box<str>, ResDesc> = desc_json["Resources"]
        .members()
        .map(|val| {
            let res_desc = parse_res_desc(val);
            (Box::clone(&res_desc.name), res_desc)
        })
        .collect();
    let mut tys: Box<[Type]> = desc_json["Types"]
        .members()
        .enumerate()
        .map(|(id, ty)| parse_ty(id, ty, &res))
        .collect::<Vec<_>>()
        .into_boxed_slice();

    restore_tyref(&mut tys, &mut calls);
    restore_res(&mut tys, &mut calls);

    *out = Some((Box::leak(calls), Box::leak(tys)));
}

fn parse_syscall(id: usize, val: &JsonValue) -> Syscall {
    /*
     NR: u64,
     Name: &str,
     CallName: &str,
     MissingArgs: u64,
     Args: [arg*],
     Attr: Attr,
     Ret: Null | u64,
    */

    let nr = val["NR"].as_u64().unwrap();
    let name = val["Name"].as_str().unwrap();
    let call_name = val["CallName"].as_str().unwrap();
    let missing_args = val["MissingArgs"].as_u64().unwrap();
    let params = val["Args"].members().map(parse_param).collect::<Vec<_>>();
    let ret = if JsonValue::Null == val["Ret"] {
        None
    } else {
        Some(TypeRef::Id(val["Ret"].as_usize().unwrap()))
    };
    let attr = parse_attr(&val["Attrs"]);

    Syscall {
        id,
        nr,
        name: to_boxed_str(name),
        call_name: to_boxed_str(call_name),
        missing_args,
        params: params.into_boxed_slice(),
        ret,
        attr,
        input_res: FxHashSet::default(), // fill latter.
        output_res: FxHashSet::default(),
    }
}

fn parse_param(val: &JsonValue) -> Param {
    // Name: &str, Type: u64, HasDirection: bool, Direction: u64
    let name = val["Name"].as_str().unwrap();
    let ty = val["Type"].as_usize().unwrap();
    let has_dir = val["HasDirection"].as_bool().unwrap();
    let dir = val["Direction"].as_u64().unwrap();

    Param {
        name: to_boxed_str(name),
        ty: TypeRef::Id(ty),
        dir: if has_dir { Some(Dir::from(dir)) } else { None },
    }
}

fn parse_attr(val: &JsonValue) -> SyscallAttr {
    /*
     Disabled: bool,
     Timeout: u64,
     ProgTimeout: u64,
     IgnoreReturn: bool,
     BreaksReturns: bool
    */
    let disable = val["Disabled"].as_bool().unwrap();
    let timeout = val["Timeout"].as_u64().unwrap();
    let prog_tmout = val["ProgTimeout"].as_u64().unwrap();
    let ignore_ret = val["IgnoreReturn"].as_bool().unwrap();
    let brk_ret = val["BreaksReturns"].as_bool().unwrap();

    SyscallAttr {
        disable,
        timeout,
        prog_tmout,
        ignore_ret,
        brk_ret,
    }
}

fn parse_ty(id: usize, ty_val: &JsonValue, res: &FxHashMap<Box<str>, ResDesc>) -> Type {
    /*
    Name: &str,
    Value: {
                TypeName: &str,
                TypeSize: u64,
                TypeAlign: u64,
                IsOptional: bool,
                IsVarlen: bool,
    }
    */
    let val = &ty_val["Value"];
    let ty_name = ty_val["Name"].as_str().unwrap();

    let name = val["TypeName"].as_str().unwrap();
    let sz = val["TypeSize"].as_u64().unwrap();
    let align = val["TypeAlign"].as_u64().unwrap();
    let optional = val["IsOptional"].as_bool().unwrap();
    let varlen = val["IsVarlen"].as_bool().unwrap();
    let kind = parse_ty_kind(ty_name, val, res);

    Type {
        id,
        name: to_boxed_str(name),
        sz,
        align,
        optional,
        varlen,
        kind,
    }
}

fn parse_ty_kind(
    kind: &str,
    json_desc: &JsonValue,
    res: &FxHashMap<Box<str>, ResDesc>,
) -> TypeKind {
    match kind {
        "ConstType" | "IntType" | "FlagsType" | "CsumType" | "LenType" | "ProcType" => {
            parse_int(kind, json_desc)
        }
        "ResourceType" => parse_res(json_desc, res),
        "VmaType" => parse_vma(json_desc),
        "PtrType" => parse_ptr(json_desc),
        "BufferType" => parse_buffer(json_desc),
        "ArrayType" => parse_array(json_desc),
        "StructType" => parse_struct(json_desc),
        "UnionType" => parse_union(json_desc),
        _ => unreachable!("unknown type kind: {}", kind),
    }
}

fn parse_int(kind: &str, val: &JsonValue) -> TypeKind {
    let fmt = parse_intfmt(val);
    match kind {
        "ConstType" => {
            // Val: u64,
            // IsPad: bool,
            let const_val = val["Val"].as_u64().unwrap();
            let pad = val["IsPad"].as_bool().unwrap();
            TypeKind::new_const(fmt, const_val, pad)
        }
        "IntType" => {
            // const INT_PLAIN_KIND: u64 = 0;
            const INT_RANGE_KIND: u64 = 1;
            // Kind: u64   // IntPlain, IntRange
            // RangeBegin, RangeEnd: u64
            // Align: u64,
            let align = val["Align"].as_u64().unwrap();
            let range = if val["Kind"].as_u64().unwrap() == INT_RANGE_KIND {
                let begin = val["RangeBegin"].as_u64().unwrap();
                let end = val["RangeEnd"].as_u64().unwrap();
                Some((begin, end))
            } else {
                None
            };
            TypeKind::new_int(fmt, range, align)
        }
        "FlagsType" => {
            // Vals: [u64],
            // BitMask: bool,
            let vals = val["Vals"].members().map(|v| v.as_u64().unwrap()).collect();
            let bitmask = val["BitMask"].as_bool().unwrap();
            TypeKind::new_flags(fmt, vals, bitmask)
        }
        "LenType" => {
            // BitSize: u64,
            // Offset: bool,
            // Path: [&str],
            let bit_sz = val["BitSize"].as_u64().unwrap();
            let offset = val["Offset"].as_bool().unwrap();
            let path = val["Path"].members().map(|p| p.as_str().unwrap()).collect();
            TypeKind::new_len(fmt, LenInfo::new(bit_sz, offset, path))
        }
        "CsumType" => {
            // Kind     u64
            // Buf      &str
            // Protocol u64
            let kind = CsumKind::from(val["Kind"].as_u64().unwrap());
            let buf = val["Buf"].as_str().unwrap();
            let proto = val["Protocol"].as_u64().unwrap();
            TypeKind::new_csum(fmt, kind, buf, proto)
        }
        "ProcType" => {
            // ValuesStart   u64
            // ValuesPerProc u64
            let start = val["ValuesStart"].as_u64().unwrap();
            let per = val["ValuesPerProc"].as_u64().unwrap();
            TypeKind::new_proc(fmt, start, per)
        }
        _ => unreachable!("unknown int type kind: {}", kind),
    }
}

fn parse_intfmt(val: &JsonValue) -> IntFmt {
    // IntTypeCommon
    // ArgFormat:       u64,
    // BitfieldOff:     u64,
    // BitfieldLen:     u64,
    // BitfieldUnit:    u64,
    // BitfieldUnitOff: u64,

    let fmt = val["ArgFormat"].as_u64().unwrap();
    let bitfield_off = val["BitfieldOff"].as_u64().unwrap();
    let bitfield_len = val["BitfieldLen"].as_u64().unwrap();
    let bitfield_unit = val["BitfieldUnit"].as_u64().unwrap();
    let bitfield_unit_off = val["BitfieldUnitOff"].as_u64().unwrap();

    IntFmt {
        fmt: BinFmt::from(fmt),
        bitfield_off,
        bitfield_len,
        bitfield_unit,
        bitfield_unit_off,
    }
}

fn parse_res(val: &JsonValue, res: &FxHashMap<Box<str>, ResDesc>) -> TypeKind {
    // ResourceType
    // ArgFormat:  u64,
    // Desc: Null,
    let fmt = BinFmt::from(val["ArgFormat"].as_u64().unwrap());
    let name = val["TypeName"].as_str().unwrap();
    let res_desc = &res[name];

    TypeKind::new_res(fmt, res_desc.clone())
}

fn parse_vma(val: &JsonValue) -> TypeKind {
    // VmaType
    // RangeBegin u64
    // RangeEnd   u64
    let begin = val["RangeBegin"].as_u64().unwrap();
    let end = val["RangeEnd"].as_u64().unwrap();

    TypeKind::new_vma(begin, end)
}

fn parse_ptr(val: &JsonValue) -> TypeKind {
    //  PtrType
    //  Elem    u64
    //  ElemDir u64
    let elem = val["Elem"].as_usize().unwrap();
    let dir = Dir::from(val["ElemDir"].as_u64().unwrap());

    TypeKind::new_ptr(elem, dir)
}

fn parse_buffer(val: &JsonValue) -> TypeKind {
    // // BufferType
    // Kind       u64
    // RangeBegin u64   // for BufferBlobRange kind
    // RangeEnd   u64   // for BufferBlobRange kind
    // Text       u64   // for BufferText
    // SubKind    &str
    // Values     [&str]   // possible values for BufferString kind
    // NoZ        bool     // non-zero terminated BufferString/BufferFilename
    const BUFFER_BLOB_RAND: u64 = 0;
    const BUFFER_BLOB_RANGE: u64 = 1;
    const BUFFER_STRING: u64 = 2;
    const BUFFER_FILENAME: u64 = 3;
    const BUFFER_TEXT: u64 = 4;
    let buf_kind = match val["Kind"].as_u64().unwrap() {
        BUFFER_BLOB_RAND => BufferKind::new_blob(None),
        BUFFER_BLOB_RANGE => {
            let begin = val["RangeBegin"].as_u64().unwrap();
            let end = val["RangeEnd"].as_u64().unwrap();
            BufferKind::new_blob(Some((begin, end)))
        }
        BUFFER_STRING => {
            let vals = val["Values"]
                .members()
                .map(|v| v.as_str().unwrap().as_bytes())
                .collect();
            let noz = val["NoZ"].as_bool().unwrap();
            BufferKind::new_str(vals, noz)
        }
        BUFFER_FILENAME => {
            let vals = val["Values"]
                .members()
                .map(|v| v.as_str().unwrap())
                .collect();
            let noz = val["NoZ"].as_bool().unwrap();
            BufferKind::new_fname(vals, noz)
        }
        BUFFER_TEXT => {
            let text_kind = TextKind::from(val["Text"].as_u64().unwrap());
            BufferKind::new_text(text_kind)
        }
        kind => unreachable!("unknown buffer kind: {}", kind),
    };
    let sub_kind = val["SubKind"].as_str().unwrap();

    TypeKind::new_buffer(buf_kind, sub_kind)
}

fn parse_array(val: &JsonValue) -> TypeKind {
    // Elem: u64,
    // Kind: u64,
    // RangeBegin: u64,
    // RangeEnd: u64,
    const ARRAY_RAND_LEN: u64 = 0;
    const ARRAY_RANGE_LEN: u64 = 1;
    let range = match val["Kind"].as_u64().unwrap() {
        ARRAY_RAND_LEN => None,
        ARRAY_RANGE_LEN => {
            let begin = val["RangeBegin"].as_u64().unwrap();
            let end = val["RangeEnd"].as_u64().unwrap();
            Some((begin, end))
        }
        kind => unreachable!("unknown array kind: {}", kind),
    };
    let elem = val["Elem"].as_usize().unwrap();

    TypeKind::new_array(elem, range)
}

fn parse_struct(val: &JsonValue) -> TypeKind {
    // StructType
    // Fields    [Field]
    // AlignAttr u64
    let align = val["AlignAttr"].as_u64().unwrap();
    let fields = val["Fields"].members().map(parse_field).collect();

    TypeKind::new_struct(align, fields)
}

fn parse_union(val: &JsonValue) -> TypeKind {
    // UnionType
    // Fields [Fields]
    let fields = val["Fields"].members().map(parse_field).collect();

    TypeKind::new_union(fields)
}

fn parse_field(val: &JsonValue) -> Field {
    let name = val["Name"].as_str().unwrap();
    let ty = val["Type"].as_usize().unwrap();
    let has_dir = val["HasDirection"].as_bool().unwrap();
    let dir = val["Direction"].as_u64().unwrap();

    Field {
        name: to_boxed_str(name),
        ty: TypeRef::Id(ty),
        dir: if has_dir { Some(Dir::from(dir)) } else { None },
    }
}

fn parse_res_desc(val: &JsonValue) -> ResDesc {
    // Name: &str,
    // Kind: [&str],
    // Values: [u64],

    let name = val["Name"].as_str().unwrap();
    let kinds = val["Kind"].members().map(|v| v.as_str().unwrap()).collect();
    let vals = val["Values"]
        .members()
        .map(|v| v.as_u64().unwrap())
        .collect();

    ResDesc::new(name, kinds, vals)
}

fn restore_tyref(tys: &mut [Type], calls: &mut [Syscall]) {
    // SAFETY: All types is guaranteed to be stores staticly.
    let addr: Vec<&'static Type> = tys
        .iter()
        .map(|addr| unsafe { mem::transmute(addr) })
        .collect();

    for call in calls.iter_mut() {
        for param in call.params.iter_mut() {
            let id = param.ty.as_id().unwrap();
            param.ty = TypeRef::Ref(addr[id]);
        }
        if let Some(ref mut ret) = call.ret {
            let id = ret.as_id().unwrap();
            *ret = TypeRef::Ref(addr[id]);
        }
    }

    for ty in tys.iter_mut() {
        match &mut ty.kind {
            TypeKind::Array { elem, .. } | TypeKind::Ptr { elem, .. } => {
                let id = elem.as_id().unwrap();
                *elem = TypeRef::Ref(addr[id]);
            }
            TypeKind::Struct { fields, .. } | TypeKind::Union { fields, .. } => {
                for field in fields.iter_mut() {
                    let id = field.ty.as_id().unwrap();
                    field.ty = TypeRef::Ref(addr[id]);
                }
            }
            _ => (),
        }
    }
}

fn restore_res(tys: &mut [Type], calls: &mut [Syscall]) {
    restore_fn_res(calls);
    for ty in tys.iter_mut() {
        if let TypeKind::Res { .. } = &mut ty.kind {
            todo!()
        }
    }
    todo!()
}

fn restore_fn_res(calls: &mut [Syscall]) {
    for call in calls.iter_mut() {
        let mut in_res = FxHashSet::default();
        let mut out_res = FxHashSet::default();
        for p in call.params.iter() {
            visit_ty(p.ty, p.dir.unwrap_or(Dir::In), |ty, dir| {
                if matches!(ty.kind, TypeKind::Res{..}) {
                    if dir != Dir::In {
                        out_res.insert(ty);
                    } else {
                        in_res.insert(ty);
                    }
                }
            })
        }
        if let Some(ret) = call.ret {
            out_res.insert(ret);
        }
        call.input_res = in_res;
        call.output_res = out_res;
    }
}

fn visit_ty<F>(ty: TypeRef, dir: Dir, mut f: F)
where
    F: FnMut(TypeRef, Dir),
{
    let mut visited: FxHashSet<TypeRef> = FxHashSet::default();
    visit_ty_inner(ty, dir, &mut f, &mut visited)
}

fn visit_ty_inner<F>(ty: TypeRef, dir: Dir, f: &mut F, visited: &mut FxHashSet<TypeRef>)
where
    F: FnMut(TypeRef, Dir),
{
    f(ty, dir);
    match &ty.kind {
        TypeKind::Ptr { elem, dir } => visit_ty_inner(*elem, *dir, f, visited),
        TypeKind::Array { elem, .. } => visit_ty_inner(*elem, dir, f, visited),
        TypeKind::Struct { fields, .. } | TypeKind::Union { fields, .. } => {
            if visited.insert(ty) {
                for field in fields.iter() {
                    visit_ty_inner(field.ty, field.dir.unwrap_or(dir), f, visited)
                }
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::targets::sys_json::*;

    #[test]
    fn parse_json_desc() {
        for (target, desc) in &TARGETS {
            let desc_json = json::parse(desc).unwrap();
            parse(target, &desc_json);
        }
    }
}
