//! AST of Syzlang type system
//!
//! Each type in syzlang has an corresponding type, while the top level `Type` wraps all of them.
//! Every type is constructed via builder pattern.
//! Everything is immutable so that value and type are always consistent.

#[macro_use]
pub mod common;
pub mod buffer;
pub mod group;
pub mod int;
pub mod ptr;
pub mod res;

use std::fmt::Display;

use crate::value::Value;
pub use buffer::*;
pub use common::*;
pub use group::*;
pub use int::*;
pub use ptr::*;
pub use res::*;

/// Dispatch method to underlying type's impl
macro_rules! dispatch{
    ($func: ident( $($arg_name: ident : $arg_ty: ty),* ) $(-> $ret:ty)?) => {
        pub fn $func(&self,  $($arg_name: $arg_ty)*) $(-> $ret)?{
                match &self.inner {
                    TypeKindInner::ResType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::ConstType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::IntType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::FlagsType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::LenType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::ProcType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::CsumType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::VmaType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::BufferBlobType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::BufferStringType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::BufferFilenameType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::ArrayType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::PtrType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::StructType(inner) => inner.$func($($arg_name)*),
                    TypeKindInner::UnionType(inner) => inner.$func($($arg_name)*),
                }
        }
    }
}

/// Impl `From`
macro_rules! impl_from_for {
    ($($kind: ident),*) => {
        $(
            impl From<$kind> for Type{
                fn from(value: $kind) -> Self{
                    Self{
                        comm: value.comm().clone(),
                        inner: TypeKindInner::$kind(value)
                    }
                }
            }
        )*
    };
}

/// Cheap conversion to underlying type
macro_rules! as_kind {
    ($as: ident, $checked_as: ident, $kind_ty: ident) => {
        pub fn $as(&self) -> Option<&$kind_ty> {
            if let TypeKindInner::$kind_ty(inner) = &self.inner {
                Some(inner)
            } else {
                None
            }
        }

        pub fn $checked_as(&self) -> &$kind_ty {
            self.$as().unwrap()
        }
    };
}

#[derive(Debug, Clone)]
pub struct Type {
    comm: CommonInfo,
    inner: TypeKindInner,
}

impl Type {
    common_attr_getter! {}

    as_kind!(as_res, checked_as_res, ResType);

    as_kind!(as_const, checked_as_const, ConstType);

    as_kind!(as_int, checked_as_int, IntType);

    as_kind!(as_flags, checked_as_flags, FlagsType);

    as_kind!(as_len, checked_as_len, LenType);

    as_kind!(as_proc, checked_as_proc, ProcType);

    as_kind!(as_csum, checked_as_csum, CsumType);

    as_kind!(as_vma, checked_as_vma, VmaType);

    as_kind!(as_buffer_blob, checked_as_buffer_blob, BufferBlobType);

    as_kind!(as_buffer_string, checked_as_buffer_string, BufferStringType);

    as_kind!(
        as_buffer_filename,
        checked_as_buffer_filename,
        BufferFilenameType
    );

    as_kind!(as_array, checked_as_array, ArrayType);

    as_kind!(as_ptr, checked_as_ptr, PtrType);

    as_kind!(as_struct, checked_as_struct, StructType);

    as_kind!(as_union, checked_as_union, UnionType);

    dispatch!(bitfield_off() -> u64);

    dispatch!(bitfield_len() -> u64);

    dispatch!(bitfield_unit() -> u64);

    dispatch!(bitfield_unit_off() -> u64);

    dispatch!(is_bitfield() -> bool);

    dispatch!(default_value(dir: Dir) -> Value);

    dispatch!(is_default(val: &Value) -> bool);

    #[inline]
    pub fn kind(&self) -> TypeKind {
        self.inner.kind()
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            TypeKindInner::ResType(inner) => inner.fmt(f),
            TypeKindInner::ConstType(inner) => inner.fmt(f),
            TypeKindInner::IntType(inner) => inner.fmt(f),
            TypeKindInner::FlagsType(inner) => inner.fmt(f),
            TypeKindInner::LenType(inner) => inner.fmt(f),
            TypeKindInner::ProcType(inner) => inner.fmt(f),
            TypeKindInner::CsumType(inner) => inner.fmt(f),
            TypeKindInner::VmaType(inner) => inner.fmt(f),
            TypeKindInner::BufferBlobType(inner) => inner.fmt(f),
            TypeKindInner::BufferStringType(inner) => inner.fmt(f),
            TypeKindInner::BufferFilenameType(inner) => inner.fmt(f),
            TypeKindInner::ArrayType(inner) => inner.fmt(f),
            TypeKindInner::PtrType(inner) => inner.fmt(f),
            TypeKindInner::StructType(inner) => inner.fmt(f),
            TypeKindInner::UnionType(inner) => inner.fmt(f),
        }
    }
}

eq_ord_hash_impl!(Type);

impl_from_for!(
    ResType,
    ConstType,
    IntType,
    FlagsType,
    LenType,
    ProcType,
    CsumType,
    VmaType,
    BufferBlobType,
    BufferStringType,
    BufferFilenameType,
    ArrayType,
    PtrType,
    StructType,
    UnionType
);

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
enum TypeKindInner {
    ResType(ResType),
    ConstType(ConstType),
    IntType(IntType),
    FlagsType(FlagsType),
    LenType(LenType),
    ProcType(ProcType),
    CsumType(CsumType),
    VmaType(VmaType),
    BufferBlobType(BufferBlobType),
    BufferStringType(BufferStringType),
    BufferFilenameType(BufferFilenameType),
    ArrayType(ArrayType),
    PtrType(PtrType),
    StructType(StructType),
    UnionType(UnionType),
}

impl TypeKindInner {
    pub fn kind(&self) -> TypeKind {
        use TypeKind::*;

        match self {
            Self::ResType(_) => Res,
            Self::ConstType(_) => Const,
            Self::IntType(_) => Int,
            Self::FlagsType(_) => Flags,
            Self::LenType(_) => Len,
            Self::ProcType(_) => Proc,
            Self::CsumType(_) => Csum,
            Self::VmaType(_) => Vma,
            Self::BufferBlobType(_) => BufferBlob,
            Self::BufferStringType(_) => BufferString,
            Self::BufferFilenameType(_) => BufferFilename,
            Self::ArrayType(_) => Array,
            Self::PtrType(_) => Ptr,
            Self::StructType(_) => Struct,
            Self::UnionType(_) => Union,
        }
    }
}

/// Type kind.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeKind {
    Res = 0,
    Const,
    Int,
    Flags,
    Len,
    Proc,
    Csum,
    Vma,
    BufferBlob,
    BufferString,
    BufferFilename,
    Array,
    Ptr,
    Struct,
    Union,
}

pub type TypeId = usize;

/// Binary format.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy)]
pub enum BinaryFormat {
    Native = 0,
    BigEndian,
    StrDec,
    StrHex,
    StrOct,
}

impl Default for BinaryFormat {
    fn default() -> Self {
        BinaryFormat::Native
    }
}

impl From<u64> for BinaryFormat {
    fn from(f: u64) -> Self {
        match f {
            0 => Self::Native,
            1 => Self::BigEndian,
            2 => Self::StrDec,
            3 => Self::StrHex,
            4 => Self::StrOct,
            _ => unreachable!(),
        }
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy, Hash)]
pub enum Dir {
    In = 0,
    Out,
    InOut,
}

impl From<u64> for Dir {
    fn from(val: u64) -> Self {
        match val {
            0 => Dir::In,
            1 => Dir::Out,
            2 => Dir::InOut,
            _ => panic!("bad dir value: {}", val),
        }
    }
}

impl std::fmt::Display for Dir {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let p = match self {
            Self::In => "In",
            Self::Out => "Out",
            Self::InOut => "InOut",
        };
        write!(f, "{}", p)
    }
}
