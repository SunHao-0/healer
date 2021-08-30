use crate::{
    ty::{common::CommonInfo, Dir, Type},
    value::{GroupValue, UnionValue, Value},
};
use std::{fmt::Display, ops::RangeInclusive};

#[derive(Debug, Clone)]
pub struct ArrayType {
    comm: CommonInfo,
    elem: Box<Type>,
    range: Option<RangeInclusive<u64>>,
}

impl ArrayType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    #[inline(always)]
    pub fn format(&self) -> crate::ty::BinaryFormat {
        crate::ty::BinaryFormat::Native
    }

    #[inline(always)]
    pub fn elem(&self) -> &Type {
        &self.elem
    }

    #[inline(always)]
    pub fn range(&self) -> Option<RangeInclusive<u64>> {
        self.range.clone()
    }

    pub fn fixed_len(&self) -> bool {
        if let Some(range) = self.range.as_ref() {
            if range.start() == range.end() {
                return true;
            }
        }
        false
    }

    pub fn default_value(&self, dir: Dir) -> Value {
        let mut elems = Vec::new();
        if let Some(range) = self.range.as_ref() {
            if range.start() == range.end() {
                elems = vec![self.elem.default_value(dir); *range.start() as usize];
            }
        }
        GroupValue::new(self.id(), dir, elems).into()
    }

    pub fn is_default(&self, val: &Value) -> bool {
        let val = val.checked_as_group();
        if !self.fixed_len() {
            return val.inner.is_empty();
        }
        val.inner.iter().all(|v| self.elem.is_default(v))
    }
}

impl Display for ArrayType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "array[{}", self.elem)?;
        if let Some(range) = self.range.as_ref() {
            if range.start() == range.end() {
                write!(f, ", {}", range.start())?;
            } else {
                write!(f, ", {}:{}", range.start(), range.end())?;
            }
        }
        write!(f, "]")
    }
}

eq_ord_hash_impl!(ArrayType);

#[derive(Debug, Clone)]
pub struct ArrayTypeBuilder {
    comm: CommonInfo,
    elem: Option<Box<Type>>,
    range: Option<RangeInclusive<u64>>,
}

impl ArrayTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            elem: None,
            range: None,
        }
    }

    pub fn elem(&mut self, elem: Type) -> &mut Self {
        self.elem = Some(Box::new(elem));
        self
    }

    pub fn range(&mut self, range: RangeInclusive<u64>) -> &mut Self {
        self.range = Some(range);
        self
    }

    pub fn build(self) -> ArrayType {
        ArrayType {
            comm: self.comm,
            elem: self.elem.unwrap(),
            range: self.range,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    /// Name of Field.
    name: Box<str>,
    /// TypeRef of Field.
    ty: Box<Type>,
    /// Direction of field.
    dir: Option<Dir>,
}

impl Field {
    pub fn new(name: String, ty: Type) -> Self {
        Self {
            name: name.into_boxed_str(),
            ty: Box::new(ty),
            dir: None,
        }
    }

    pub fn set_dir(&mut self, dir: Dir) {
        self.dir = Some(dir);
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline(always)]
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    #[inline(always)]
    pub fn dir(&self) -> Option<Dir> {
        self.dir
    }
}

impl Display for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.name, self.ty.name())
    }
}

#[derive(Debug, Clone)]
pub struct StructType {
    comm: CommonInfo,
    fields: Box<[Field]>,
    align_attr: u64,
}

impl StructType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    #[inline(always)]
    pub fn format(&self) -> crate::ty::BinaryFormat {
        crate::ty::BinaryFormat::Native
    }

    #[inline(always)]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    #[inline(always)]
    pub fn align_attr(&self) -> u64 {
        self.align_attr
    }

    pub fn default_value(&self, dir: Dir) -> Value {
        let mut inner = Vec::with_capacity(self.fields.len());
        for f in self.fields.iter() {
            inner.push(f.ty.default_value(dir));
        }
        GroupValue::new(self.id(), dir, inner).into()
    }

    pub fn is_default(&self, val: &Value) -> bool {
        let val = val.checked_as_group();
        self.fields
            .iter()
            .zip(val.inner.iter())
            .all(|(f, v)| f.ty.is_default(v))
    }
}

impl Display for StructType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "struct {} {{", self.name())?;
        for (i, field) in self.fields.iter().enumerate() {
            write!(f, "{}", field)?;
            if i != self.fields.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, "}}")
    }
}

eq_ord_hash_impl!(StructType);

#[derive(Debug, Clone)]
pub struct StructTypeBuilder {
    comm: CommonInfo,
    fields: Vec<Field>,
    align_attr: u64,
}

impl StructTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            fields: Vec::new(),
            align_attr: 0,
        }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn fields(&mut self, fields: Vec<Field>) -> &mut Self {
        self.fields = fields;
        self
    }

    pub fn align_attr(&mut self, align_attr: u64) -> &mut Self {
        self.align_attr = align_attr;
        self
    }

    pub fn build(self) -> StructType {
        StructType {
            comm: self.comm,
            fields: self.fields.into_boxed_slice(),
            align_attr: self.align_attr,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnionType {
    comm: CommonInfo,
    fields: Box<[Field]>,
}

impl UnionType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    #[inline(always)]
    pub fn format(&self) -> crate::ty::BinaryFormat {
        crate::ty::BinaryFormat::Native
    }

    #[inline(always)]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn default_value(&self, dir: Dir) -> Value {
        let inner = self.fields[0].ty.default_value(dir);
        UnionValue::new(self.id(), dir, 0, inner).into()
    }

    pub fn is_default(&self, val: &Value) -> bool {
        let val = val.checked_as_union();
        val.index == 0 && self.fields[0].ty.is_default(&val.option)
    }
}

impl Display for UnionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "union {} {{", self.name())?;
        for (i, field) in self.fields.iter().enumerate() {
            write!(f, "{}", field)?;
            if i != self.fields.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, "}}")
    }
}

eq_ord_hash_impl!(UnionType);

#[derive(Debug, Clone)]
pub struct UnionTypeBuilder {
    comm: CommonInfo,
    fields: Vec<Field>,
}

impl UnionTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            fields: Vec::new(),
        }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn fields(&mut self, fields: Vec<Field>) -> &mut Self {
        self.fields = fields;
        self
    }

    pub fn build(self) -> UnionType {
        UnionType {
            comm: self.comm,
            fields: self.fields.into_boxed_slice(),
        }
    }
}
