use std::fmt::Display;

use crate::{
    ty::{common::CommonInfo, BinaryFormat, Dir},
    value::{ResValue, ResValueKind, Value},
};

pub type ResKind = Box<str>;

#[derive(Debug, Clone)]
pub struct ResType {
    comm: CommonInfo,
    bin_fmt: BinaryFormat,
    /// Name of resource.
    name: Box<str>,
    /// Subkind of these kind resource.
    kinds: Box<[ResKind]>,
    /// Special value for current resource.
    vals: Box<[u64]>,
}

impl ResType {
    common_attr_getter! {}

    default_int_format_attr_getter! {}

    #[inline(always)]
    pub fn format(&self) -> BinaryFormat {
        self.bin_fmt
    }

    #[inline(always)]
    pub fn res_name(&self) -> &ResKind {
        &self.name
    }

    #[inline(always)]
    pub fn kinds(&self) -> &[Box<str>] {
        &self.kinds
    }

    #[inline(always)]
    pub fn special_vals(&self) -> &[u64] {
        &self.vals
    }

    pub fn default_special_val(&self) -> u64 {
        self.vals.get(0).copied().unwrap_or_default()
    }

    pub fn default_value(&self, dir: Dir) -> Value {
        ResValue::new_null(self.id(), dir, self.default_special_val()).into()
    }

    pub fn is_default(&self, val: &Value) -> bool {
        let val = val.checked_as_res();

        if let ResValueKind::Null = &val.kind {
            val.val == self.default_special_val() && val.op_add == 0 && val.op_div == 0
        } else {
            false
        }
    }

    pub fn is_subtype_of(&self, other: &ResType) -> bool {
        if other.kinds.len() > self.kinds.len() {
            return false;
        }
        let kinds = &self.kinds[..other.kinds.len()];
        kinds == &other.kinds[..]
    }
}

impl Display for ResType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "resource[{}]", self.name)
    }
}

eq_ord_hash_impl!(ResType);

#[derive(Debug, Clone)]
pub struct ResTypeBuilder {
    comm: CommonInfo,
    bin_fmt: BinaryFormat,
    name: String,
    kinds: Vec<String>,
    vals: Vec<u64>,
}

impl ResTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            name: String::new(),
            bin_fmt: BinaryFormat::Native,
            kinds: Vec::new(),
            vals: Vec::new(),
        }
    }

    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    pub fn bin_fmt(&mut self, fmt: BinaryFormat) -> &mut Self {
        self.bin_fmt = fmt;
        self
    }

    pub fn res_name<T: Into<String>>(&mut self, name: T) -> &mut Self {
        self.name = name.into();
        self
    }

    pub fn kinds(&mut self, kinds: Vec<String>) -> &mut Self {
        self.kinds = kinds;
        self
    }

    pub fn vals(&mut self, vals: Vec<u64>) -> &mut Self {
        self.vals = vals;
        self
    }

    pub fn build(self) -> ResType {
        ResType {
            comm: self.comm,
            name: self.name.into_boxed_str(),
            bin_fmt: self.bin_fmt,
            kinds: self
                .kinds
                .into_iter()
                .map(|kind| kind.into_boxed_str())
                .collect(),
            vals: self.vals.into_boxed_slice(),
        }
    }
}
