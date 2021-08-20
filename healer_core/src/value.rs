use crate::{
    target::Target,
    ty::{Dir, ProcType, PtrType, Type, TypeId, TypeKind},
};
use std::{ascii::escape_default, fmt::Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueCommon {
    pub ty_id: TypeId,
    pub dir: Dir,
}

impl ValueCommon {
    pub fn new(ty_id: TypeId, dir: Dir) -> Self {
        Self { ty_id, dir }
    }

    pub fn ty<'a>(&self, target: &'a Target) -> &'a Type {
        target.ty_of(self.ty_id)
    }

    pub fn ty_id(&self) -> TypeId {
        self.ty_id
    }
}

macro_rules! common_attr_getter {
    () => {
        #[inline(always)]
        pub fn ty<'a>(&self, target: &'a Target) -> &'a Type {
            self.comm.ty(target)
        }

        #[inline(always)]
        pub fn ty_id(&self) -> TypeId {
            self.comm.ty_id()
        }

        #[inline(always)]
        pub fn dir(&self) -> Dir {
            self.comm.dir
        }

        pub fn layout(&self, target: &Target) -> core::alloc::Layout {
            let sz = self.size(target);
            let ty = self.ty(target);
            let align = ty.align();
            let align = if align == 0 { 1 } else { align };

            core::alloc::Layout::from_size_align(sz as usize, align as usize).unwrap()
        }
    };
}

/// Dispatch method to underlying type's impl
macro_rules! dispatch{
    ($func: ident( $($arg_name: ident : $arg_ty: ty),* ) $(-> $ret:ty)?) => {
        pub fn $func(&self,  $($arg_name: $arg_ty)*) $(-> $ret)?{
                match &self.inner {
                    ValueKindInner::IntegerValue(inner) => inner.$func($($arg_name)*),
                    ValueKindInner::PtrValue(inner) => inner.$func($($arg_name)*),
                    ValueKindInner::VmaValue(inner) => inner.$func($($arg_name)*),
                    ValueKindInner::DataValue(inner) => inner.$func($($arg_name)*),
                    ValueKindInner::GroupValue(inner) => inner.$func($($arg_name)*),
                    ValueKindInner::UnionValue(inner) => inner.$func($($arg_name)*),
                    ValueKindInner::ResValue(inner) => inner.$func($($arg_name)*),
                }
        }
    }
}

/// Impl `From`
macro_rules! impl_from_for {
    ($($kind: ident),*) => {
        $(
            impl From<$kind> for Value{
                fn from(value: $kind) -> Self{
                    Self{
                        comm: value.comm,
                        inner: ValueKindInner::$kind(value)
                    }
                }
            }
        )*
    };
}

/// Cheap conversion to underlying type
macro_rules! as_kind {
    ($as: ident, $as_mut: ident, $checked_as: ident, $checked_as_mut: ident, $kind_ty: ident) => {
        #[inline]
        pub fn $as(&self) -> Option<&$kind_ty> {
            if let ValueKindInner::$kind_ty(inner) = &self.inner {
                Some(inner)
            } else {
                None
            }
        }

        #[inline]
        pub fn $as_mut(&mut self) -> Option<&mut $kind_ty> {
            if let ValueKindInner::$kind_ty(inner) = &mut self.inner {
                Some(inner)
            } else {
                None
            }
        }

        #[inline(always)]
        pub fn $checked_as(&self) -> &$kind_ty {
            self.$as().unwrap()
        }

        #[inline(always)]
        pub fn $checked_as_mut(&mut self) -> &mut $kind_ty {
            self.$as_mut().unwrap()
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Value {
    pub comm: ValueCommon,
    inner: ValueKindInner,
}

impl Value {
    dispatch!(size(target: &Target) -> u64);
    dispatch!(layout(target: &Target) -> core::alloc::Layout);

    as_kind!(
        as_int,
        as_int_mut,
        checked_as_int,
        checked_as_int_mut,
        IntegerValue
    );

    as_kind!(
        as_ptr,
        as_ptr_mut,
        checked_as_ptr,
        checked_as_ptr_mut,
        PtrValue
    );

    as_kind!(
        as_vma,
        as_vma_mut,
        checked_as_vma,
        checked_as_vma_mut,
        VmaValue
    );

    as_kind!(
        as_data,
        as_data_mut,
        checked_as_data,
        checked_as_data_mut,
        DataValue
    );

    as_kind!(
        as_group,
        as_group_mut,
        checked_as_group,
        checked_as_group_mut,
        GroupValue
    );

    as_kind!(
        as_union,
        as_union_mut,
        checked_as_union,
        checked_as_union_mut,
        UnionValue
    );

    as_kind!(
        as_res,
        as_res_mut,
        checked_as_res,
        checked_as_res_mut,
        ResValue
    );

    #[inline(always)]
    pub fn ty<'a>(&self, target: &'a Target) -> &'a Type {
        self.comm.ty(target)
    }

    #[inline(always)]
    pub fn ty_id(&self) -> TypeId {
        self.comm.ty_id()
    }

    #[inline(always)]
    pub fn dir(&self) -> Dir {
        self.comm.dir
    }

    #[inline]
    pub fn kind(&self) -> ValueKind {
        self.inner.kind()
    }

    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> ValueDisplay<'a, 'b> {
        ValueDisplay { val: self, target }
    }
}

#[derive(Debug, Clone)]
pub struct ValueDisplay<'a, 'b> {
    val: &'a Value,
    target: &'b Target,
}

impl<'a, 'b> Display for ValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.val.inner {
            ValueKindInner::IntegerValue(inner) => write!(f, "{}", inner.display(self.target)),
            ValueKindInner::PtrValue(inner) => write!(f, "{}", inner.display(self.target)),
            ValueKindInner::VmaValue(inner) => write!(f, "{}", inner.display(self.target)),
            ValueKindInner::DataValue(inner) => write!(f, "{}", inner.display(self.target)),
            ValueKindInner::GroupValue(inner) => write!(f, "{}", inner.display(self.target)),
            ValueKindInner::UnionValue(inner) => write!(f, "{}", inner.display(self.target)),
            ValueKindInner::ResValue(inner) => write!(f, "{}", inner.display(self.target)),
        }
    }
}

impl_from_for!(
    IntegerValue,
    PtrValue,
    VmaValue,
    DataValue,
    GroupValue,
    UnionValue,
    ResValue
);

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ValueKindInner {
    IntegerValue(IntegerValue),
    PtrValue(PtrValue),
    VmaValue(VmaValue),
    DataValue(DataValue),
    GroupValue(GroupValue),
    UnionValue(UnionValue),
    ResValue(ResValue),
}

impl ValueKindInner {
    pub fn kind(&self) -> ValueKind {
        use ValueKind::*;
        match self {
            Self::IntegerValue(_) => Integer,
            Self::PtrValue(_) => Ptr,
            Self::VmaValue(_) => Vma,
            Self::DataValue(_) => Data,
            Self::GroupValue(_) => Group,
            Self::UnionValue(_) => Union,
            Self::ResValue(_) => Res,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueKind {
    Integer,
    Ptr,
    Vma,
    Data,
    Group,
    Union,
    Res,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntegerValue {
    pub comm: ValueCommon,
    pub val: u64,
}

impl IntegerValue {
    common_attr_getter! {}

    pub fn new(ty: TypeId, dir: Dir, val: u64) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            val,
        }
    }

    #[inline(always)]
    pub fn size(&self, target: &Target) -> u64 {
        self.ty(target).size()
    }

    pub fn value(&self, target: &Target) -> (u64, u64) {
        let ty = self.ty(target);
        match ty.kind() {
            TypeKind::Proc => {
                let ty = ty.checked_as_proc();
                if self.val == ProcType::PROC_DEFAULT_VALUE {
                    (0, 0)
                } else {
                    (ty.values_start() + self.val, ty.values_per_proc())
                }
            }
            _ => (self.val, 0),
        }
    }

    pub fn display<'a, 'b>(&'a self, _target: &'b Target) -> IntegerValueDisplay<'a, 'b> {
        IntegerValueDisplay { val: self, _target }
    }
}

#[derive(Debug, Clone)]
pub struct IntegerValueDisplay<'a, 'b> {
    val: &'a IntegerValue,
    _target: &'b Target,
}

impl<'a, 'b> Display for IntegerValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.val)
    }
}

impl Display for IntegerValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#x}", self.val)
    }
}

pub type PtrAddress = u64;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PtrValue {
    pub comm: ValueCommon,
    pub addr: PtrAddress, // address may exist while pointee is none
    pub pointee: Option<Box<Value>>,
}

impl PtrValue {
    common_attr_getter! {}

    pub fn new(ty: TypeId, dir: Dir, addr: PtrAddress, data: Value) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            addr,
            pointee: Some(Box::new(data)),
        }
    }

    #[inline(always)]
    pub fn size(&self, target: &Target) -> u64 {
        self.ty(target).size()
    }

    pub fn new_special(ty: TypeId, dir: Dir, index: u64) -> Self {
        assert!(index < PtrType::MAX_SPECIAL_POINTERS);

        Self {
            comm: ValueCommon::new(ty, dir),
            addr: -(index as i64) as u64,
            pointee: None,
        }
    }

    pub fn is_special(&self) -> bool {
        self.pointee.is_none() && -(self.addr as i64) < (PtrType::MAX_SPECIAL_POINTERS as i64)
    }

    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> PtrValueDisplay<'a, 'b> {
        PtrValueDisplay { val: self, target }
    }
}

pub const ENCODING_ADDR_BASE: u64 = 0x7f0000000000;

#[derive(Debug, Clone)]
pub struct PtrValueDisplay<'a, 'b> {
    val: &'a PtrValue,
    target: &'b Target,
}

impl<'a, 'b> Display for PtrValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = self.val;
        if val.is_special() {
            return write!(f, "{:#x}", val.addr);
        }
        let addr = val.addr + ENCODING_ADDR_BASE;
        write!(f, "&({:#x})=", addr)?;
        if let Some(pointee) = val.pointee.as_ref() {
            write!(f, "{}", pointee.display(self.target))
        } else {
            write!(f, "nil")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VmaValue {
    pub comm: ValueCommon,
    pub addr: PtrAddress,
    pub vma_size: u64,
}

impl VmaValue {
    common_attr_getter! {}

    pub fn new(ty: TypeId, dir: Dir, addr: u64, vma_size: u64) -> Self {
        assert_eq!(addr % 1024, 0);

        Self {
            comm: ValueCommon::new(ty, dir),
            addr,
            vma_size,
        }
    }

    #[inline(always)]
    pub fn size(&self, target: &Target) -> u64 {
        self.ty(target).size()
    }

    pub fn new_special(ty: TypeId, dir: Dir, index: u64) -> Self {
        assert!(index < PtrType::MAX_SPECIAL_POINTERS);

        Self {
            comm: ValueCommon::new(ty, dir),
            addr: -(index as i64) as u64,
            vma_size: 0,
        }
    }

    pub fn is_special(&self) -> bool {
        self.vma_size == 0 && -(self.addr as i64) < (PtrType::MAX_SPECIAL_POINTERS as i64)
    }

    pub fn display<'a, 'b>(&'a self, _target: &'b Target) -> VmaValueDisplay<'a, 'b> {
        VmaValueDisplay { val: self, _target }
    }
}

#[derive(Debug, Clone)]
pub struct VmaValueDisplay<'a, 'b> {
    val: &'a VmaValue,
    _target: &'b Target,
}

impl<'a, 'b> Display for VmaValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.val)
    }
}

impl Display for VmaValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_special() {
            return write!(f, "{:#x}", self.addr);
        }
        let addr = self.addr + ENCODING_ADDR_BASE;
        write!(f, "&({:#x}/{:#x})=nil", addr, self.vma_size)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DataValue {
    pub comm: ValueCommon,
    pub data: Vec<u8>,
    pub size: u64, // for out args
}

impl DataValue {
    common_attr_getter! {}

    pub fn new(ty: TypeId, dir: Dir, data: Vec<u8>) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            data,
            size: 0,
        }
    }

    pub fn new_out_data(ty: TypeId, dir: Dir, size: u64) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            data: Vec::new(),
            size,
        }
    }

    pub fn size(&self, _target: &Target) -> u64 {
        if self.data.is_empty() {
            self.size
        } else {
            self.data.len() as u64
        }
    }

    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> DataValueDisplay<'a, 'b> {
        DataValueDisplay { val: self, target }
    }
}

#[derive(Debug, Clone)]
pub struct DataValueDisplay<'a, 'b> {
    val: &'a DataValue,
    target: &'b Target,
}

impl<'a, 'b> Display for DataValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = self.val;
        let ty = val.ty(self.target);
        let mut data = &val.data[..];

        // mark size for output data
        if val.dir() == Dir::Out {
            return write!(f, "\"\"/{:#x}", val.size);
        }
        // try to shrink
        while data.len() >= 2 && data[data.len() - 1] == 0 && data[data.len() - 2] == 0 {
            data = &data[..data.len() - 1];
        }
        if ty.varlen() && data.len() + 8 >= val.data.len() {
            data = &val.data[..];
        }
        // display
        if !matches!(ty.kind(), TypeKind::BufferString | TypeKind::BufferFilename)
            && (data.is_empty() || !is_readable(data))
        {
            write!(f, "\"{}\"", encode_hex(data))?;
        } else {
            let val = data
                .iter()
                .copied()
                .flat_map(escape_default)
                .collect::<Vec<_>>();
            let val = String::from_utf8(val).unwrap();
            write!(f, "\'{}\'", val)?;
        }
        // mark size if we dropped ch for varlen type
        if ty.varlen() && data.len() != val.data.len() {
            write!(f, "/{}", val.data.len())?;
        }
        Ok(())
    }
}

fn encode_hex(val: &[u8]) -> String {
    const HEX: [char; 16] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    let mut ret = String::with_capacity(val.len() * 2);
    for v in val.iter().copied() {
        ret.push(HEX[(v >> 4) as usize]);
        ret.push(HEX[((v & 0x0f) as usize)]);
    }
    ret
}

#[allow(clippy::match_like_matches_macro)]
fn is_readable(data: &[u8]) -> bool {
    data.iter().all(|v| match *v {
        0 | 0x7 | 0x8 | 0xC | 0xA | 0xD | b'\t' | 0xB => true,
        0x20..=0x7e => true,
        _ => false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupValue {
    pub comm: ValueCommon,
    pub inner: Vec<Value>,
}

impl GroupValue {
    common_attr_getter! {}

    pub fn new(ty: TypeId, dir: Dir, data: Vec<Value>) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            inner: data,
        }
    }

    pub fn size(&self, target: &Target) -> u64 {
        let ty = self.ty(target);
        // if !ty.varlen() {
        // return ty.size();
        // }
        let mut size = 0;
        for f in self.inner.iter() {
            size += f.size(target);
        }
        if let TypeKind::Struct = ty.kind() {
            let ty = ty.checked_as_struct();
            if ty.align_attr() != 0 && size % ty.align_attr() != 0 {
                size += ty.align_attr() - size % ty.align_attr();
            }
        }
        size
    }

    pub fn fixed_inner_size(&self, target: &Target) -> bool {
        let ty = self.ty(target);
        match ty.kind() {
            TypeKind::Struct => true,
            TypeKind::Array => {
                let ty = ty.checked_as_array();
                if let Some(range) = ty.range() {
                    *range.start() == *range.end()
                } else {
                    false
                }
            }
            _ => unreachable!(),
        }
    }

    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> GroupValueDisplay<'a, 'b> {
        GroupValueDisplay { val: self, target }
    }
}

#[derive(Debug, Clone)]
pub struct GroupValueDisplay<'a, 'b> {
    val: &'a GroupValue,
    target: &'b Target,
}

impl<'a, 'b> Display for GroupValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = self.val;
        let ty = val.ty(self.target);
        let delims = if ty.kind() == TypeKind::Array {
            ['[', ']']
        } else {
            ['{', '}']
        };
        write!(f, "{}", delims[0])?;
        // strip default
        let mut last_non_default = val.inner.len();
        if val.fixed_inner_size(self.target) {
            while last_non_default != 0 {
                let inner_val = &val.inner[last_non_default - 1];
                let inner_ty = inner_val.ty(self.target);
                if !inner_ty.is_default(inner_val) {
                    break;
                }
                last_non_default -= 1;
            }
        }
        for i in 0..last_non_default {
            let inner_val = &val.inner[i];
            let inner_ty = inner_val.ty(self.target);
            if inner_ty.kind() == TypeKind::Const && inner_ty.checked_as_const().pad() {
                continue;
            }
            write!(f, "{}", inner_val.display(self.target))?;
            if i != last_non_default - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, "{}", delims[1])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnionValue {
    pub comm: ValueCommon,
    pub option: Box<Value>,
    pub index: u64,
}

impl UnionValue {
    common_attr_getter! {}

    pub fn new(ty: TypeId, dir: Dir, index: u64, option: Value) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            option: Box::new(option),
            index,
        }
    }

    pub fn size(&self, target: &Target) -> u64 {
        let ty = self.ty(target);
        if !ty.varlen() {
            ty.size()
        } else {
            self.option.size(target)
        }
    }

    pub fn display<'a, 'b>(&'a self, target: &'b Target) -> UnionValueDisplay<'a, 'b> {
        UnionValueDisplay { val: self, target }
    }
}

#[derive(Debug, Clone)]
pub struct UnionValueDisplay<'a, 'b> {
    val: &'a UnionValue,
    target: &'b Target,
}

impl<'a, 'b> Display for UnionValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = self.val;
        let ty = val.ty(self.target);
        let ty = ty.checked_as_union();
        let field = &ty.fields()[val.index as usize];

        write!(f, "@{}", field.name())?;
        if !field.ty().is_default(&val.option) {
            write!(f, "={}", val.option.display(self.target))?;
        }
        Ok(())
    }
}

pub type ResValueId = usize;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResValue {
    pub comm: ValueCommon,
    pub kind: ResValueKind,
    pub val: u64,
    pub op_div: u64,
    pub op_add: u64,
}

impl ResValue {
    common_attr_getter! {}

    #[inline(always)]
    pub fn size(&self, target: &Target) -> u64 {
        self.ty(target).size()
    }

    pub fn new_ref(ty: TypeId, dir: Dir, res_id: ResValueId) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            kind: ResValueKind::Ref(res_id),
            val: 0,
            op_div: 0,
            op_add: 0,
        }
    }

    pub fn new_res(ty: TypeId, res_id: ResValueId, val: u64) -> Self {
        Self {
            comm: ValueCommon::new(ty, Dir::Out),
            kind: ResValueKind::Own(res_id),
            val,
            op_div: 0,
            op_add: 0,
        }
    }

    pub fn new_null(ty: TypeId, dir: Dir, val: u64) -> Self {
        Self {
            comm: ValueCommon::new(ty, dir),
            kind: ResValueKind::Null,
            val,
            op_div: 0,
            op_add: 0,
        }
    }

    pub fn ref_res(&self) -> bool {
        matches!(self.kind, ResValueKind::Ref(..))
    }

    pub fn own_res(&self) -> bool {
        matches!(self.kind, ResValueKind::Own(..))
    }

    pub fn is_null(&self) -> bool {
        matches!(self.kind, ResValueKind::Null)
    }

    pub fn res_val_id(&self) -> Option<ResValueId> {
        match &self.kind {
            ResValueKind::Own(id) | ResValueKind::Ref(id) => Some(*id),
            _ => None,
        }
    }

    pub fn display<'a, 'b>(&'a self, _target: &'b Target) -> ResValueDisplay<'a, 'b> {
        ResValueDisplay { val: self, _target }
    }
}

#[derive(Debug, Clone)]
pub struct ResValueDisplay<'a, 'b> {
    val: &'a ResValue,
    _target: &'b Target,
}

impl<'a, 'b> Display for ResValueDisplay<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = self.val;

        match val.kind {
            ResValueKind::Own(id) => {
                write!(f, "<r{}=>{:#x}", id, val.val)
            }
            ResValueKind::Ref(id) => {
                write!(f, "r{}", id)?;
                if val.op_div != 0 {
                    write!(f, "/{}", val.op_div)?;
                }
                if val.op_add != 0 {
                    write!(f, "+{}", val.op_add)?;
                }
                Ok(())
            }
            ResValueKind::Null => write!(f, "{:#x}", val.val),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResValueKind {
    Own(ResValueId),
    Ref(ResValueId),
    Null,
}
