use crate::{
    ty::{common::CommonInfo, Dir},
    value::{DataValue, Value},
};
use std::{fmt::Display, ops::RangeInclusive};

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy, Hash)]
pub enum TextKind {
    Target = 0,
    X86Real,
    X86bit16,
    X86bit32,
    X86bit64,
    Arm64,
    Ppc64,
}

impl From<u64> for TextKind {
    fn from(kind: u64) -> Self {
        match kind {
            0 => Self::Target,
            1 => Self::X86Real,
            2 => Self::X86bit16,
            3 => Self::X86bit32,
            4 => Self::X86bit64,
            5 => Self::Arm64,
            6 => Self::Ppc64,
            _ => unreachable!(),
        }
    }
}

macro_rules! buffer_type_default {
    () => {
        pub fn default_value(&self, dir: Dir) -> Value {
            if dir == Dir::Out {
                let sz = if self.varlen() { 0 } else { self.size() };
                DataValue::new_out_data(self.id(), dir, sz).into()
            } else {
                let val = if !self.vals.is_empty() {
                    Vec::from(&self.vals[0][..])
                } else if !self.varlen() {
                    vec![0; self.size() as usize]
                } else {
                    Vec::new()
                };
                DataValue::new(self.id(), dir, val).into()
            }
        }

        pub fn is_default(&self, val: &Value) -> bool {
            let val = val.checked_as_data();
            let sz = if !self.varlen() { self.size() } else { 0 };
            if val.data.len() as u64 != sz {
                return false;
            }
            if val.dir() == Dir::Out {
                return true;
            }
            if !self.vals.is_empty() {
                return &self.vals[0][..] == &val.data[..];
            }
            val.data.iter().all(|v| *v == 0)
        }
    };
}

#[derive(Debug, Clone)]
pub struct BufferBlobType {
    comm: CommonInfo,
    range: Option<RangeInclusive<u64>>,
    sub_kind: Option<Box<str>>,
    vals: Box<[Box<[u8]>]>,
    text_kind: Option<TextKind>,
}

impl BufferBlobType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    buffer_type_default! {}

    #[inline(always)]
    pub fn range(&self) -> Option<RangeInclusive<u64>> {
        self.range.clone()
    }

    #[inline(always)]
    pub fn sub_kind(&self) -> Option<&str> {
        self.sub_kind.as_deref()
    }

    #[inline(always)]
    pub fn vals(&self) -> &[Box<[u8]>] {
        &self.vals
    }

    #[inline(always)]
    pub fn text_kind(&self) -> Option<TextKind> {
        self.text_kind
    }
}

impl Display for BufferBlobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.text_kind.is_some() {
            write!(f, "text")
        } else {
            write!(f, "buffer")
        }
    }
}

eq_ord_hash_impl!(BufferBlobType);

#[derive(Debug, Clone)]
pub struct BufferBlobTypeBuilder {
    comm: CommonInfo,
    range: Option<RangeInclusive<u64>>,
    sub_kind: Option<String>,
    vals: Vec<Vec<u8>>,
    text_kind: Option<TextKind>,
}

impl BufferBlobTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            range: None,
            sub_kind: None,
            vals: Vec::new(),
            text_kind: None,
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn range(&mut self, range: RangeInclusive<u64>) -> &mut Self {
        self.range = Some(range);
        self
    }

    pub fn sub_kind<T: Into<String>>(&mut self, sub_kind: T) -> &mut Self {
        let sub_kind: String = sub_kind.into();
        if !sub_kind.is_empty() {
            self.sub_kind = Some(sub_kind);
        }
        self
    }

    pub fn vals(&mut self, vals: Vec<Vec<u8>>) -> &mut Self {
        self.vals = vals;
        self
    }

    pub fn text_kind<T: Into<TextKind>>(&mut self, text_kind: T) -> &mut Self {
        self.text_kind = Some(text_kind.into());
        self
    }

    pub fn build(self) -> BufferBlobType {
        BufferBlobType {
            comm: self.comm,
            range: self.range,
            sub_kind: self.sub_kind.map(|sub_kind| sub_kind.into_boxed_str()),
            vals: self
                .vals
                .into_iter()
                .map(|v| v.into_boxed_slice())
                .collect(),
            text_kind: self.text_kind,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferStringType {
    comm: CommonInfo,
    sub_kind: Option<Box<str>>,
    vals: Box<[Box<[u8]>]>,
    noz: bool,
    is_glob: bool,
}

impl BufferStringType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    buffer_type_default! {}

    #[inline(always)]
    pub fn sub_kind(&self) -> Option<&str> {
        self.sub_kind.as_deref()
    }

    #[inline(always)]
    pub fn vals(&self) -> &[Box<[u8]>] {
        &self.vals
    }

    #[inline(always)]
    pub fn noz(&self) -> bool {
        self.noz
    }

    #[inline(always)]
    pub fn is_glob(&self) -> bool {
        self.is_glob
    }
}

impl Display for BufferStringType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.noz {
            write!(f, "stringnoz")
        } else if self.is_glob {
            write!(f, "glob")
        } else {
            write!(f, "string")
        }
    }
}

eq_ord_hash_impl!(BufferStringType);

#[derive(Debug, Clone)]
pub struct BufferStringTypeBuilder {
    comm: CommonInfo,
    sub_kind: Option<String>,
    vals: Vec<Vec<u8>>,
    noz: bool,
    is_glob: bool,
}

impl BufferStringTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            sub_kind: None,
            vals: Vec::new(),
            noz: false,
            is_glob: false,
        }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn sub_kind<T: Into<String>>(&mut self, sub_kind: T) -> &mut Self {
        let sub_kind: String = sub_kind.into();
        if !sub_kind.is_empty() {
            self.sub_kind = Some(sub_kind);
        }
        self
    }

    pub fn vals(&mut self, vals: Vec<Vec<u8>>) -> &mut Self {
        self.vals = vals;
        self
    }

    pub fn noz(&mut self, noz: bool) -> &mut Self {
        self.noz = noz;
        self
    }

    pub fn is_glob(&mut self, is_glob: bool) -> &mut Self {
        self.is_glob = is_glob;
        self
    }

    pub fn build(self) -> BufferStringType {
        BufferStringType {
            comm: self.comm,
            sub_kind: self.sub_kind.map(|v| v.into_boxed_str()),
            vals: self
                .vals
                .into_iter()
                .map(|v| v.into_boxed_slice())
                .collect(),
            noz: self.noz,
            is_glob: self.is_glob,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferFilenameType {
    comm: CommonInfo,
    vals: Box<[Box<[u8]>]>, // not used yet
    noz: bool,
}

impl BufferFilenameType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    extra_attr_getter! {}

    buffer_type_default! {}

    #[inline(always)]
    pub fn vals(&self) -> &[Box<[u8]>] {
        &self.vals
    }

    #[inline(always)]
    pub fn noz(&self) -> bool {
        self.noz
    }
}

impl Display for BufferFilenameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "filename")
    }
}

eq_ord_hash_impl!(BufferFilenameType);

#[derive(Debug, Clone)]
pub struct BufferFilenameTypeBuilder {
    comm: CommonInfo,
    vals: Vec<Vec<u8>>,
    noz: bool,
}

impl BufferFilenameTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            vals: Vec::new(),
            noz: false,
        }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn vals(&mut self, vals: Vec<Vec<u8>>) -> &mut Self {
        self.vals = vals;
        self
    }

    pub fn noz(&mut self, noz: bool) -> &mut Self {
        self.noz = noz;
        self
    }

    pub fn build(self) -> BufferFilenameType {
        BufferFilenameType {
            comm: self.comm,
            vals: self
                .vals
                .into_iter()
                .map(|v| v.into_boxed_slice())
                .collect(),
            noz: self.noz,
        }
    }
}
