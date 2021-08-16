use super::JsonValue;
use crate::{
    sys::{get, LoadError},
    HashMap, HashSet,
};
use healer_core::{
    syscall::{Syscall, SyscallAttr, SyscallBuilder},
    ty::{
        ArrayTypeBuilder, BinaryFormat, BufferBlobTypeBuilder, BufferFilenameTypeBuilder,
        BufferStringTypeBuilder, CommonInfo, CommonInfoBuilder, ConstTypeBuilder, CsumTypeBuilder,
        Field, FlagsTypeBuilder, IntFormat, IntTypeBuilder, LenTypeBuilder, ProcTypeBuilder,
        PtrTypeBuilder, ResTypeBuilder, StructTypeBuilder, Type, TypeId, UnionTypeBuilder,
        VmaTypeBuilder,
    },
};
use simd_json::prelude::*;

/// Convert json description to ast
///
/// Json format: {"Syscalls": [{Syscall Object}, ...], "Types": [{Type Object}, ...], "Resources": [{Resources}, ...]}
/// Steps: Resources -> Types -> Syscalls
#[allow(clippy::type_complexity)]
pub fn description_json_to_ast(
    description: &JsonValue,
) -> Result<(Vec<Syscall>, Vec<Type>, Vec<String>), LoadError> {
    let res_info_js = get(description, "Resources")?.as_array().unwrap();
    let mut res_info_mapping: HashMap<String, ResourceInfo> = HashMap::default(); // name -> info
    for res in res_info_js {
        let res_info = convert_resource_json(res)?;
        res_info_mapping.insert(res_info.name.clone(), res_info);
    }
    let mut res_kinds = res_info_mapping
        .iter()
        .map(|(kind, _)| kind.to_string())
        .collect::<HashSet<_>>();
    let tys_json = get(description, "Types")?.as_array().unwrap();
    let tys = convert_tys_json(tys_json, &res_info_mapping)?;
    for ty in tys.iter().filter_map(|(_, ty)| ty.as_res()) {
        for kind in ty.kinds() {
            res_kinds.insert(kind.to_string());
        }
    }
    let mut res_kinds = res_kinds.into_iter().collect::<Vec<_>>();
    res_kinds.sort_unstable();

    let syscalls_json = get(description, "Syscalls")?.as_array().unwrap();
    let syscalls = convert_syscall_json(syscalls_json, &tys)?;
    let mut tys = tys.into_iter().map(|(_, ty)| ty).collect::<Vec<_>>();
    tys.sort_unstable();
    Ok((syscalls, tys, res_kinds))
}

#[derive(Clone, Debug)]
struct ResourceInfo {
    pub name: String,
    pub kind: Vec<String>,
    pub vals: Vec<u64>,
}

fn convert_resource_json(res: &JsonValue) -> Result<ResourceInfo, LoadError> {
    let name = get(res, "Name")?.as_str().unwrap().to_string();
    let mut kind = get(res, "Kind")?
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    kind.sort_unstable();
    let mut vals = get(res, "Values")?
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect::<Vec<_>>();
    vals.sort_unstable();
    let res_info = ResourceInfo { name, kind, vals };
    Ok(res_info)
}

fn convert_tys_json(
    tys: &[JsonValue],
    res_infos: &HashMap<String, ResourceInfo>,
) -> Result<HashMap<TypeId, Type>, LoadError> {
    struct TypeConvertContext<'a> {
        tid: usize,
        ty: &'a JsonValue,
        converted: bool,
    }

    let mut converted: HashMap<TypeId, Type> = HashMap::default();
    let mut tys_ctx: Vec<TypeConvertContext> = tys
        .iter()
        .enumerate()
        .map(|(tid, ty)| TypeConvertContext {
            tid,
            ty,
            converted: false,
        })
        .collect();
    let mut left = tys.len();

    while left != 0 {
        for ty_ctx in tys_ctx.iter_mut().filter(|ty| !ty.converted) {
            if let Some(ty) = try_convert_ty((ty_ctx.tid, ty_ctx.ty), &converted, res_infos)? {
                converted.insert(ty_ctx.tid, ty);
                ty_ctx.converted = true;
                left -= 1;
            }
        }
    }
    assert_eq!(converted.len(), tys.len());

    Ok(converted)
}

fn convert_syscall_json(
    syscalls_json: &[JsonValue],
    tys: &HashMap<TypeId, Type>,
) -> Result<Vec<Syscall>, LoadError> {
    let mut syscalls = Vec::with_capacity(syscalls_json.len());
    for (id, syscall_js) in syscalls_json.iter().enumerate() {
        let mut builder = SyscallBuilder::new();
        builder
            .id(id)
            .nr(get(syscall_js, "NR")?.as_u64().unwrap())
            .name(get(syscall_js, "Name")?.as_str().unwrap())
            .call_name(get(syscall_js, "CallName")?.as_str().unwrap())
            .missing_args(get(syscall_js, "MissingArgs")?.as_u64().unwrap());
        let params_js = get(syscall_js, "Args")?
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut params = Vec::with_capacity(params_js.len());
        for param_js in params_js {
            let param = convert_field(&param_js, tys)?
                .ok_or_else(|| LoadError::Parse(format!("missting field: {:?}", param_js)))?;
            params.push(param);
        }
        builder.params(params);
        if !get(syscall_js, "Ret")?.is_null() {
            let ret = get(syscall_js, "Ret")?.as_u64().unwrap() as usize;
            let ret = tys.get(&ret).ok_or_else(|| {
                LoadError::Parse(format!("missing type: {}", get(syscall_js, "Ret").unwrap()))
            })?;
            builder.ret(ret.clone());
        }
        builder.attr(convert_syscall_attr_json(get(syscall_js, "Attrs")?)?);
        syscalls.push(builder.build())
    }
    Ok(syscalls)
}

fn convert_syscall_attr_json(val: &JsonValue) -> Result<SyscallAttr, LoadError> {
    let disabled = get(val, "Disabled")?.as_bool().unwrap();
    let timeout = get(val, "Timeout")?.as_u64().unwrap();
    let prog_timeout = get(val, "ProgTimeout")?.as_u64().unwrap();
    let ignore_return = get(val, "IgnoreReturn")?.as_bool().unwrap();
    let breaks_returns = get(val, "BreaksReturns")?.as_bool().unwrap();
    let attr = SyscallAttr {
        disabled,
        timeout,
        prog_timeout,
        ignore_return,
        breaks_returns,
    };
    Ok(attr)
}

const KNOWN_KINDS: [&str; 13] = [
    "ConstType",
    "IntType",
    "FlagsType",
    "CsumType",
    "LenType",
    "ProcType",
    "ResourceType",
    "VmaType",
    "PtrType",
    "BufferType",
    "ArrayType",
    "StructType",
    "UnionType",
];

type KindConvertor = fn(
    (TypeId, &JsonValue),
    &HashMap<TypeId, Type>,
    &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError>;
const KIND_CONVERTERS: [KindConvertor; 13] = [
    convert_const_ty,
    convert_int_ty,
    convert_flags_ty,
    convert_csum_ty,
    convert_len_ty,
    convert_proc_ty,
    convert_res_ty,
    convert_vma_ty,
    convert_ptr_ty,
    convert_buffer_ty,
    convert_array_ty,
    convert_struct_ty,
    convert_union_ty,
];

fn try_convert_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    converted: &HashMap<TypeId, Type>,
    res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let ty_info_js = get(ty_js, "Value")?;
    let ty_kind = get(ty_js, "Name")?.as_str().unwrap().trim();
    let idx = KNOWN_KINDS
        .iter()
        .position(|kind| *kind == ty_kind)
        .ok_or_else(|| LoadError::Parse(format!("unknown kind: {}", ty_kind)))?;
    KIND_CONVERTERS[idx]((tid, ty_info_js), converted, res_infos)
}

fn convert_const_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let fmt = convert_intfmt(ty_js)?;
    let mut builder = ConstTypeBuilder::new(comm);
    builder
        .int_fmt(fmt)
        .const_val(get(ty_js, "Val")?.as_u64().unwrap())
        .pad(get(ty_js, "IsPad")?.as_bool().unwrap());
    Ok(Some(builder.build().into()))
}

fn convert_int_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    const INT_PLAIN: u64 = 0;
    const INT_RANGE: u64 = 1;

    let comm = convert_comm((tid, ty_js))?;
    let fmt = convert_intfmt(ty_js)?;
    let mut builder = IntTypeBuilder::new(comm);
    builder
        .int_fmt(fmt)
        .align(get(ty_js, "Align")?.as_u64().unwrap());
    match get(ty_js, "Kind")?.as_u64().unwrap() {
        INT_PLAIN => (),
        INT_RANGE => {
            let start = get(ty_js, "RangeBegin")?.as_u64().unwrap();
            let end = get(ty_js, "RangeEnd")?.as_u64().unwrap();
            if start as i64 >= 0 {
                assert!(end >= start, "end: {}, start: {}", end, start);
            }
            builder.range(std::ops::RangeInclusive::new(start, end));
        }
        unknown_kind => {
            return Err(LoadError::Parse(format!(
                "unknown int kind: {}",
                unknown_kind
            )))
        }
    }
    Ok(Some(builder.build().into()))
}

fn convert_flags_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let fmt = convert_intfmt(ty_js)?;
    let mut builder = FlagsTypeBuilder::new(comm);
    let vals = get(ty_js, "Vals")?
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    builder
        .int_fmt(fmt)
        .vals(vals)
        .bit_mask(get(ty_js, "BitMask")?.as_bool().unwrap());
    Ok(Some(builder.build().into()))
}

fn convert_len_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let fmt = convert_intfmt(ty_js)?;
    let path = get(ty_js, "Path")?
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let mut builder = LenTypeBuilder::new(comm);
    builder
        .int_fmt(fmt)
        .bit_size(get(ty_js, "BitSize")?.as_u64().unwrap())
        .offset(get(ty_js, "Offset")?.as_bool().unwrap())
        .path(path);
    Ok(Some(builder.build().into()))
}

fn convert_csum_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let fmt = convert_intfmt(ty_js)?;
    let mut builder = CsumTypeBuilder::new(comm);
    builder
        .int_fmt(fmt)
        .kind(get(ty_js, "Kind")?.as_u64().unwrap().into())
        .buf(get(ty_js, "Buf")?.as_str().unwrap())
        .protocol(get(ty_js, "Protocol")?.as_u64().unwrap());

    Ok(Some(builder.build().into()))
}

fn convert_proc_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let fmt = convert_intfmt(ty_js)?;
    let mut builder = ProcTypeBuilder::new(comm);
    builder
        .int_fmt(fmt)
        .values_start(get(ty_js, "ValuesStart")?.as_u64().unwrap())
        .values_per_proc(get(ty_js, "ValuesPerProc")?.as_u64().unwrap());
    Ok(Some(builder.build().into()))
}

fn convert_res_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let fmt: BinaryFormat = get(ty_js, "ArgFormat")?.as_u64().unwrap().into();
    let name = get(ty_js, "TypeName")?.as_str().unwrap();
    let res_info = res_infos
        .get(name)
        .ok_or_else(|| LoadError::Parse(format!("missing resource: {}", name)))?
        .clone();
    let mut builder = ResTypeBuilder::new(comm);
    builder
        .bin_fmt(fmt)
        .res_name(res_info.name)
        .kinds(res_info.kind)
        .vals(res_info.vals);
    Ok(Some(builder.build().into()))
}

const BUFFER_BLOB_RAND: u64 = 0;
const BUFFER_BLOB_RANGE: u64 = 1;
const BUFFER_STRING: u64 = 2;
const BUFFER_FILENAME: u64 = 3;
const BUFFER_TEXT: u64 = 4;
const BUFFER_GLOB: u64 = 5;

fn convert_buffer_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    match get(ty_js, "Kind")?.as_u64().unwrap() {
        kind if kind == BUFFER_BLOB_RAND || kind == BUFFER_BLOB_RANGE || kind == BUFFER_TEXT => {
            convert_buffer_blob((tid, ty_js), kind)
        }
        kind if kind == BUFFER_STRING || kind == BUFFER_GLOB => {
            convert_buffer_string((tid, ty_js), kind)
        }
        BUFFER_FILENAME => convert_buffer_filename((tid, ty_js)),
        unknown_kind => Err(LoadError::Parse(format!(
            "unknown buffer kind: {}",
            unknown_kind
        ))),
    }
}

fn convert_buffer_string(
    (tid, ty_js): (TypeId, &JsonValue),
    kind: u64,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let mut builder = BufferStringTypeBuilder::new(comm);
    if let Some(vals) = get(ty_js, "Values")?.as_array() {
        let vals = vals
            .iter()
            .map(|v| v.as_str().unwrap().as_bytes().to_vec())
            .collect();
        builder.vals(vals);
    }
    builder
        .noz(get(ty_js, "NoZ")?.as_bool().unwrap())
        .sub_kind(get(ty_js, "SubKind")?.as_str().unwrap())
        .is_glob(kind == BUFFER_GLOB);
    Ok(Some(builder.build().into()))
}

fn convert_buffer_filename((tid, ty_js): (TypeId, &JsonValue)) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let mut builder = BufferFilenameTypeBuilder::new(comm);
    if let Some(vals) = get(ty_js, "Values")?.as_array() {
        let vals = vals
            .iter()
            .map(|v| v.as_str().unwrap().as_bytes().to_vec())
            .collect();
        builder.vals(vals);
    }
    builder.noz(get(ty_js, "NoZ")?.as_bool().unwrap());

    Ok(Some(builder.build().into()))
}

fn convert_buffer_blob(
    (tid, ty_js): (TypeId, &JsonValue),
    kind: u64,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;

    let mut builder = BufferBlobTypeBuilder::new(comm);
    if kind == BUFFER_BLOB_RANGE {
        let start = get(ty_js, "RangeBegin")?.as_u64().unwrap();
        let end = get(ty_js, "RangeEnd")?.as_u64().unwrap();
        assert!(end >= start, "end: {}, start: {}", end, start);
        builder.range(std::ops::RangeInclusive::new(start, end));
    }
    if kind == BUFFER_TEXT {
        builder.text_kind(get(ty_js, "Text")?.as_u64().unwrap());
    }
    if let Some(vals) = get(ty_js, "Values")?.as_array() {
        let vals = vals
            .iter()
            .map(|v| v.as_str().unwrap().as_bytes().to_vec())
            .collect();
        builder.vals(vals);
    }
    builder.sub_kind(get(ty_js, "SubKind")?.as_str().unwrap());
    Ok(Some(builder.build().into()))
}

fn convert_ptr_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let mut builder = PtrTypeBuilder::new(comm);
    builder
        .dir(get(ty_js, "ElemDir")?.as_u64().unwrap())
        .elem(get(ty_js, "Elem")?.as_u64().unwrap() as usize);
    Ok(Some(builder.build().into()))
}

fn convert_vma_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    _converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let start = get(ty_js, "RangeBegin")?.as_u64().unwrap();
    let end = get(ty_js, "RangeEnd")?.as_u64().unwrap();
    let mut builder = VmaTypeBuilder::new(comm);
    if start != 0 || end != 0 {
        assert!(end >= start, "end: {}, start: {}", end, start);
        builder.range(std::ops::RangeInclusive::new(start, end));
    }
    Ok(Some(builder.build().into()))
}

fn convert_array_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    const _ARRAY_RAND_LEN: u64 = 0;
    const ARRAY_RANGE_LEN: u64 = 1;

    let comm = convert_comm((tid, ty_js))?;
    let mut builder = ArrayTypeBuilder::new(comm);
    let kind = get(ty_js, "Kind")?.as_u64().unwrap();
    if kind == ARRAY_RANGE_LEN {
        let start = get(ty_js, "RangeBegin")?.as_u64().unwrap();
        let end = get(ty_js, "RangeEnd")?.as_u64().unwrap();
        assert!(end >= start, "end: {}, start: {}", end, start);
        builder.range(std::ops::RangeInclusive::new(start, end));
    }
    let elem_ty_id = get(ty_js, "Elem")?.as_u64().unwrap() as usize;
    if let Some(ty) = converted.get(&elem_ty_id) {
        builder.elem(ty.clone());
        Ok(Some(builder.build().into()))
    } else {
        Ok(None)
    }
}

fn convert_struct_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let mut builder = StructTypeBuilder::new(comm);
    let mut fields = Vec::new();
    let fields_js = get(ty_js, "Fields")?.as_array().unwrap();
    for field_js in fields_js {
        if let Some(field) = convert_field(field_js, converted)? {
            fields.push(field);
        } else {
            // field not converted yet, wait
            return Ok(None);
        }
    }
    builder
        .fields(fields)
        .align_attr(get(ty_js, "AlignAttr")?.as_u64().unwrap());
    Ok(Some(builder.build().into()))
}

fn convert_union_ty(
    (tid, ty_js): (TypeId, &JsonValue),
    converted: &HashMap<TypeId, Type>,
    _res_infos: &HashMap<String, ResourceInfo>,
) -> Result<Option<Type>, LoadError> {
    let comm = convert_comm((tid, ty_js))?;
    let mut builder = UnionTypeBuilder::new(comm);
    let mut fields = Vec::new();
    let fields_js = get(ty_js, "Fields")?.as_array().unwrap();
    for field_js in fields_js {
        if let Some(field) = convert_field(field_js, converted)? {
            fields.push(field);
        } else {
            // field not converted yet, wait
            return Ok(None);
        }
    }
    builder.fields(fields);
    Ok(Some(builder.build().into()))
}

fn convert_field(
    field_js: &JsonValue,
    converted: &HashMap<TypeId, Type>,
) -> Result<Option<Field>, LoadError> {
    let name = get(field_js, "Name")?.as_str().unwrap();
    let has_dir = get(field_js, "HasDirection")?.as_bool().unwrap();
    let dir = get(field_js, "Direction")?.as_u64().unwrap();
    let ty_id = get(field_js, "Type")?.as_u64().unwrap() as usize;

    if let Some(ty) = converted.get(&ty_id) {
        let mut field = Field::new(name.to_string(), ty.clone());
        if has_dir {
            field.set_dir(dir.into());
        }
        Ok(Some(field))
    } else {
        Ok(None)
    }
}

fn convert_comm((tid, ty_js): (TypeId, &JsonValue)) -> Result<CommonInfo, LoadError> {
    let mut builder = CommonInfoBuilder::new();
    builder
        .id(tid)
        .name(get(ty_js, "TypeName")?.as_str().unwrap())
        .size(get(ty_js, "TypeSize")?.as_u64().unwrap())
        .align(get(ty_js, "TypeAlign")?.as_u64().unwrap())
        .optional(get(ty_js, "IsOptional")?.as_bool().unwrap())
        .varlen(get(ty_js, "IsVarlen")?.as_bool().unwrap());
    Ok(builder.build())
}

fn convert_intfmt(val: &JsonValue) -> Result<IntFormat, LoadError> {
    let fmt = get(val, "ArgFormat")?.as_u64().unwrap();
    let bitfield_off = get(val, "BitfieldOff")?.as_u64().unwrap();
    let bitfield_len = get(val, "BitfieldLen")?.as_u64().unwrap();
    let bitfield_unit = get(val, "BitfieldUnit")?.as_u64().unwrap();
    let bitfield_unit_off = get(val, "BitfieldUnitOff")?.as_u64().unwrap();
    Ok(IntFormat {
        fmt: BinaryFormat::from(fmt),
        bitfield_off,
        bitfield_len,
        bitfield_unit,
        bitfield_unit_off,
    })
}
