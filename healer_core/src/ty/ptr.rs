use crate::{
    ty::{common::CommonInfo, Dir},
    value::{PtrValue, Value, VmaValue},
};
use std::{fmt::Display, ops::RangeInclusive};

use super::TypeId;

#[derive(Debug, Clone)]
pub struct VmaType {
    comm: CommonInfo,
    range: Option<RangeInclusive<u64>>,
}

impl VmaType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    #[inline(always)]
    pub fn format(&self) -> crate::ty::BinaryFormat {
        crate::ty::BinaryFormat::Native
    }

    #[inline(always)]
    pub fn range(&self) -> Option<RangeInclusive<u64>> {
        self.range.clone()
    }

    pub fn default_value(&self, dir: Dir) -> Value {
        VmaValue::new_special(self.id(), dir, 0).into()
    }

    pub fn is_default(&self, val: &Value) -> bool {
        let val = val.checked_as_vma();
        val.is_special() && val.addr == 0
    }
}

impl Display for VmaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vma")?;
        if let Some(range) = self.range.as_ref() {
            if range.start() == range.end() {
                write!(f, "[{}]", range.start())?;
            } else {
                write!(f, "[{}:{}]", range.start(), range.end())?;
            }
        }
        Ok(())
    }
}

eq_ord_hash_impl!(VmaType);

#[derive(Debug, Clone)]
pub struct VmaTypeBuilder {
    comm: CommonInfo,
    range: Option<RangeInclusive<u64>>,
}

impl VmaTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self { comm, range: None }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn range(&mut self, range: RangeInclusive<u64>) -> &mut Self {
        self.range = Some(range);
        self
    }

    pub fn build(self) -> VmaType {
        VmaType {
            comm: self.comm,
            range: self.range,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PtrType {
    comm: CommonInfo,
    elem: TypeId, // handle recursive type
    dir: Dir,
}

impl PtrType {
    pub const MAX_SPECIAL_POINTERS: u64 = 16;

    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    #[inline(always)]
    pub fn format(&self) -> crate::ty::BinaryFormat {
        crate::ty::BinaryFormat::Native
    }

    #[inline(always)]
    pub fn elem(&self) -> TypeId {
        self.elem
    }

    #[inline(always)]
    pub fn dir(&self) -> Dir {
        self.dir
    }

    pub fn default_value(&self, dir: Dir) -> Value {
        // we don't have `target` here, so just give a null
        PtrValue::new_special(self.id(), dir, 0).into()
    }

    pub fn is_default(&self, val: &Value) -> bool {
        let val = val.checked_as_ptr();
        val.is_special()
    }
}

impl Display for PtrType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ptr[{:?}, {}]", self.dir, self.elem)
    }
}

eq_ord_hash_impl!(PtrType);

#[derive(Debug, Clone)]
pub struct PtrTypeBuilder {
    comm: CommonInfo,
    elem: Option<TypeId>,
    dir: Dir,
}

impl PtrTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            elem: None,
            dir: Dir::In,
        }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn elem(&mut self, elem: TypeId) -> &mut Self {
        self.elem = Some(elem);
        self
    }

    pub fn dir<T: Into<Dir>>(&mut self, dir: T) -> &mut Self {
        self.dir = dir.into();
        self
    }

    pub fn build(self) -> PtrType {
        PtrType {
            comm: self.comm,
            elem: self.elem.unwrap(),
            dir: self.dir,
        }
    }
}
