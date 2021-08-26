use crate::{
    ty::{common::CommonInfo, BinaryFormat, Dir},
    value::{IntegerValue, Value},
};
use std::{fmt::Display, ops::RangeInclusive};

/// Integer format.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy, Default)]
pub struct IntFormat {
    pub fmt: BinaryFormat,
    pub bitfield_off: u64,
    pub bitfield_len: u64,
    pub bitfield_unit: u64,
    pub bitfield_unit_off: u64,
}

macro_rules! int_format_attr_getter {
    () => {
        #[inline(always)]
        pub fn format(&self) -> crate::ty::BinaryFormat {
            self.int_fmt.fmt
        }

        #[inline(always)]
        pub fn bitfield_off(&self) -> u64 {
            self.int_fmt.bitfield_off
        }

        #[inline(always)]
        pub fn bitfield_len(&self) -> u64 {
            self.int_fmt.bitfield_len
        }

        #[inline(always)]
        pub fn bitfield_unit(&self) -> u64 {
            // self.int_fmt.bitfield_unit
            if self.bitfield_len() != 0 {
                // self.bitfield_unit()
                self.int_fmt.bitfield_unit
            } else {
                self.size()
            }
        }

        // pub fn unit_size(&self) -> u64 {
        //     if self.bitfield_len() != 0 {
        //         self.bitfield_unit()
        //     } else {
        //         self.size()
        //     }
        // }

        #[inline(always)]
        pub fn bitfield_unit_off(&self) -> u64 {
            self.int_fmt.bitfield_unit_off
        }

        pub fn bit_size(&self) -> u64 {
            if let crate::ty::BinaryFormat::Native | crate::ty::BinaryFormat::BigEndian =
                self.format()
            {
                if self.bitfield_len() != 0 {
                    self.bitfield_len()
                } else {
                    self.size() * 8
                }
            } else {
                64
            }
        }

        #[inline(always)]
        pub fn is_bitfield(&self) -> bool {
            self.bitfield_len() != 0
        }
    };
}

macro_rules! default_int_value {
    () => {
        #[inline]
        pub fn default_value(&self, dir: Dir) -> Value {
            IntegerValue::new(self.id(), dir, self.default_special_value()).into()
        }

        #[inline(always)]
        pub fn is_default(&self, val: &Value) -> bool {
            val.checked_as_int().val == self.default_special_value()
        }
    };
}

#[derive(Debug, Clone)]
pub struct ConstType {
    comm: CommonInfo,
    int_fmt: IntFormat,
    const_val: u64,
    pad: bool,
}

impl ConstType {
    common_attr_getter! {}

    int_format_attr_getter! {}

    default_int_value! {}

    #[inline(always)]
    pub fn int_fmt(&self) -> &IntFormat {
        &self.int_fmt
    }

    #[inline(always)]
    pub fn const_val(&self) -> u64 {
        self.const_val
    }

    #[inline(always)]
    pub fn pad(&self) -> bool {
        self.pad
    }

    #[inline(always)]
    pub fn default_special_value(&self) -> u64 {
        self.const_val
    }
}

impl Display for ConstType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "const[{}]", self.const_val)
    }
}

eq_ord_hash_impl!(ConstType);

#[derive(Debug, Clone)]
pub struct ConstTypeBuilder {
    comm: CommonInfo,
    int_fmt: IntFormat,
    const_val: u64,
    pad: bool,
}

impl ConstTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            const_val: 0,
            pad: false,
            int_fmt: IntFormat::default(),
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    #[inline(always)]
    pub fn int_fmt(&mut self, fmt: IntFormat) -> &mut Self {
        self.int_fmt = fmt;
        self
    }

    #[inline(always)]
    pub fn const_val(&mut self, const_val: u64) -> &mut Self {
        self.const_val = const_val;
        self
    }

    #[inline(always)]
    pub fn pad(&mut self, pad: bool) -> &mut Self {
        self.pad = pad;
        self
    }

    pub fn build(self) -> ConstType {
        ConstType {
            comm: self.comm,
            int_fmt: self.int_fmt,
            const_val: self.const_val,
            pad: self.pad,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IntType {
    comm: CommonInfo,
    int_fmt: IntFormat,
    range: Option<RangeInclusive<u64>>,
    align: u64,
}

impl IntType {
    common_attr_getter! {}

    int_format_attr_getter! {}

    default_int_value! {}

    #[inline(always)]
    pub fn int_fmt(&self) -> &IntFormat {
        &self.int_fmt
    }

    #[inline(always)]
    pub fn range(&self) -> Option<&RangeInclusive<u64>> {
        self.range.as_ref()
    }

    #[inline(always)]
    pub fn int_align(&self) -> u64 {
        self.align
    }

    #[inline(always)]
    pub fn default_special_value(&self) -> u64 {
        0
    }
}

impl Display for IntType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "int")?;
        if self.is_bitfield() {
            write!(f, ":{}", self.bitfield_len())?;
        } else if let Some(range) = self.range.as_ref() {
            write!(f, "[{}:{}", range.start(), range.end())?;
            if self.align != 0 {
                write!(f, ", {}", self.align)?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

eq_ord_hash_impl!(IntType);

#[derive(Debug, Clone)]
pub struct IntTypeBuilder {
    comm: CommonInfo,
    int_fmt: IntFormat,
    range: Option<RangeInclusive<u64>>,
    align: u64,
}

impl IntTypeBuilder {
    #[inline(always)]
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            int_fmt: IntFormat::default(),
            range: None,
            align: 0,
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    #[inline(always)]
    pub fn int_fmt(&mut self, fmt: IntFormat) -> &mut Self {
        self.int_fmt = fmt;
        self
    }

    #[inline(always)]
    pub fn range(&mut self, range: RangeInclusive<u64>) -> &mut Self {
        self.range = Some(range);
        self
    }

    #[inline(always)]
    pub fn align(&mut self, align: u64) -> &mut Self {
        self.align = align;
        self
    }

    pub fn build(self) -> IntType {
        IntType {
            comm: self.comm,
            int_fmt: self.int_fmt,
            range: self.range,
            align: self.align,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FlagsType {
    comm: CommonInfo,
    int_fmt: IntFormat,
    vals: Box<[u64]>,
    bit_mask: bool,
}

impl FlagsType {
    common_attr_getter! {}

    int_format_attr_getter! {}

    default_int_value! {}

    #[inline(always)]
    pub fn int_fmt(&self) -> &IntFormat {
        &self.int_fmt
    }

    #[inline(always)]
    pub fn vals(&self) -> &[u64] {
        &self.vals
    }

    #[inline(always)]
    pub fn bit_mask(&self) -> bool {
        self.bit_mask
    }

    #[inline(always)]
    pub fn default_special_value(&self) -> u64 {
        0
    }
}

impl Display for FlagsType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "flag[{}]", self.name())
    }
}

eq_ord_hash_impl!(FlagsType);

#[derive(Debug, Clone)]
pub struct FlagsTypeBuilder {
    comm: CommonInfo,
    int_fmt: IntFormat,
    vals: Vec<u64>,
    bit_mask: bool,
}

impl FlagsTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            int_fmt: IntFormat::default(),
            vals: Vec::new(),
            bit_mask: true,
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    #[inline(always)]
    pub fn int_fmt(&mut self, fmt: IntFormat) -> &mut Self {
        self.int_fmt = fmt;
        self
    }

    #[inline(always)]
    pub fn vals(&mut self, vals: Vec<u64>) -> &mut Self {
        self.vals = vals;
        self
    }

    #[inline(always)]
    pub fn bit_mask(&mut self, bit_mask: bool) -> &mut Self {
        self.bit_mask = bit_mask;
        self
    }

    pub fn build(self) -> FlagsType {
        FlagsType {
            comm: self.comm,
            int_fmt: self.int_fmt,
            vals: self.vals.into_boxed_slice(),
            bit_mask: self.bit_mask,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LenType {
    comm: CommonInfo,
    int_fmt: IntFormat,
    bit_size: u64,
    offset: bool,
    path: Box<[Box<str>]>,
}

impl LenType {
    common_attr_getter! {}

    int_format_attr_getter! {}

    default_int_value! {}

    #[inline(always)]
    pub fn int_fmt(&self) -> &IntFormat {
        &self.int_fmt
    }

    #[inline(always)]
    pub fn len_bit_size(&self) -> u64 {
        self.bit_size
    }

    #[inline(always)]
    pub fn offset(&self) -> bool {
        self.offset
    }

    #[inline(always)]
    pub fn path(&self) -> &[Box<str>] {
        &self.path
    }

    #[inline(always)]
    pub fn default_special_value(&self) -> u64 {
        0
    }
}

impl Display for LenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.offset {
            write!(f, "offsetof")?;
        } else if self.bit_size == 0 {
            write!(f, "len")?;
        } else if self.bit_size == 8 {
            write!(f, "bytesize")?;
        } else {
            write!(f, "bitsize")?;
        }
        write!(f, "[{}]", self.path.join("."))
    }
}

eq_ord_hash_impl!(LenType);

#[derive(Debug, Clone)]
pub struct LenTypeBuilder {
    comm: CommonInfo,
    int_fmt: IntFormat,
    bit_size: u64,
    offset: bool,
    path: Vec<String>,
}

impl LenTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            int_fmt: IntFormat::default(),
            bit_size: 64,
            offset: false,
            path: Vec::new(),
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    #[inline(always)]
    pub fn int_fmt(&mut self, fmt: IntFormat) -> &mut Self {
        self.int_fmt = fmt;
        self
    }

    #[inline(always)]
    pub fn bit_size(&mut self, bit_size: u64) -> &mut Self {
        self.bit_size = bit_size;
        self
    }

    #[inline(always)]
    pub fn offset(&mut self, offset: bool) -> &mut Self {
        self.offset = offset;
        self
    }

    #[inline(always)]
    pub fn path(&mut self, path: Vec<String>) -> &mut Self {
        self.path = path;
        self
    }

    #[inline(always)]
    pub fn build(self) -> LenType {
        LenType {
            comm: self.comm,
            int_fmt: self.int_fmt,
            bit_size: self.bit_size,
            offset: self.offset,
            path: self.path.into_iter().map(|p| p.into_boxed_str()).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcType {
    comm: CommonInfo,
    int_fmt: IntFormat,
    values_start: u64,
    values_per_proc: u64,
}

impl ProcType {
    pub const MAX_PIDS: u64 = 32;
    pub const PROC_DEFAULT_VALUE: u64 = 0xffffffffffffffff;

    common_attr_getter! {}

    int_format_attr_getter! {}

    default_int_value! {}

    #[inline(always)]
    pub fn int_fmt(&self) -> &IntFormat {
        &self.int_fmt
    }

    #[inline(always)]
    pub fn values_start(&self) -> u64 {
        self.values_start
    }

    #[inline(always)]
    pub fn values_per_proc(&self) -> u64 {
        self.values_per_proc
    }

    #[inline(always)]
    pub fn default_special_value(&self) -> u64 {
        Self::PROC_DEFAULT_VALUE
    }
}

impl Display for ProcType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "proc[{}, {}]", self.values_per_proc, self.values_start)
    }
}

eq_ord_hash_impl!(ProcType);

pub struct ProcTypeBuilder {
    comm: CommonInfo,
    int_fmt: IntFormat,
    values_start: u64,
    values_per_proc: u64,
}

impl ProcTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            int_fmt: IntFormat::default(),
            values_start: 0,
            values_per_proc: 0,
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    #[inline(always)]
    pub fn int_fmt(&mut self, fmt: IntFormat) -> &mut Self {
        self.int_fmt = fmt;
        self
    }

    #[inline(always)]
    pub fn values_start(&mut self, values_start: u64) -> &mut Self {
        self.values_start = values_start;
        self
    }

    #[inline(always)]
    pub fn values_per_proc(&mut self, values_per_proc: u64) -> &mut Self {
        self.values_per_proc = values_per_proc;
        self
    }

    pub fn build(self) -> ProcType {
        ProcType {
            comm: self.comm,
            int_fmt: self.int_fmt,
            values_start: self.values_start,
            values_per_proc: self.values_per_proc,
        }
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy, Hash)]
pub enum CsumKind {
    Inet = 0,
    Pseudo,
}

impl From<u64> for CsumKind {
    fn from(kind: u64) -> Self {
        match kind {
            0 => CsumKind::Inet,
            1 => CsumKind::Pseudo,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CsumType {
    comm: CommonInfo,
    int_fmt: IntFormat,
    kind: CsumKind,
    buf: Box<str>,
    protocol: u64,
}

impl CsumType {
    common_attr_getter! {}

    int_format_attr_getter! {}

    default_int_value! {}

    #[inline(always)]
    pub fn int_fmt(&self) -> &IntFormat {
        &self.int_fmt
    }

    #[inline(always)]
    pub fn kind(&self) -> CsumKind {
        self.kind
    }

    #[inline(always)]
    pub fn buf(&self) -> &str {
        &self.buf
    }

    #[inline(always)]
    pub fn protocol(&self) -> u64 {
        self.protocol
    }

    #[inline(always)]
    pub fn default_special_value(&self) -> u64 {
        0
    }
}

impl Display for CsumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "csum[{:?}]", self.kind)
    }
}

eq_ord_hash_impl!(CsumType);

#[derive(Debug, Clone)]
pub struct CsumTypeBuilder {
    comm: CommonInfo,
    int_fmt: IntFormat,
    kind: CsumKind,
    buf: String,
    protocol: u64,
}

impl CsumTypeBuilder {
    pub fn new(comm: CommonInfo) -> Self {
        Self {
            comm,
            int_fmt: IntFormat::default(),
            kind: CsumKind::Inet,
            buf: String::new(),
            protocol: 0,
        }
    }

    #[inline(always)]
    pub fn comm(&mut self, comm: CommonInfo) -> &mut Self {
        self.comm = comm;
        self
    }

    #[inline(always)]
    pub fn int_fmt(&mut self, fmt: IntFormat) -> &mut Self {
        self.int_fmt = fmt;
        self
    }

    #[inline(always)]
    pub fn kind(&mut self, kind: CsumKind) -> &mut Self {
        self.kind = kind;
        self
    }

    #[inline(always)]
    pub fn buf<T: Into<String>>(&mut self, buf: T) -> &mut Self {
        self.buf = buf.into();
        self
    }

    #[inline(always)]
    pub fn protocol(&mut self, protocol: u64) -> &mut Self {
        self.protocol = protocol;
        self
    }

    pub fn build(self) -> CsumType {
        CsumType {
            comm: self.comm,
            int_fmt: self.int_fmt,
            kind: self.kind,
            buf: self.buf.into_boxed_str(),
            protocol: self.protocol,
        }
    }
}
