//! Abstract representation or AST of system call model.
use std::fmt;
use std::rc::Rc;

/// System call id.
pub type SId = usize;

/// Information related to particular system call.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub struct Syscall {
    /// Index of system call, diffierent from nr.
    pub id: SId,
    /// Call number, set to 0 for system that doesn't use nr.
    pub nr: u64,
    /// System call name.
    // TODO This could be optimized. Not every syscall need store the whole name.
    pub name: Box<str>,
    /// Name of specialized system call.
    pub call_name: Box<str>,
    /// Syzkaller: Number of trailing args that should be zero-filled.
    pub miss_args: u64,
    /// Parameters of calls.
    pub params: Box<[Param]>,
    /// Return type of system call: a ref to res type or None.
    pub ret: Option<Rc<Type>>,
    /// Attributes of system call.
    pub attr: SyscallAttr,
    // /// Input result parameters.
    // in_res: FxHashSet<Box<Type>>,
    // /// Output result parameters.
    // out_res: FxHashSet<TypeId>,
    // /// Name to param map.
    // param_map: FxHashMap<Box<str>, usize>,
    // /// Hash of current entry, decided by nr and sp_name.
    // /// Must be inited properly.
    // hash: u32,
}

impl Syscall {
    pub fn new(
        id: SId,
        nr: u64,
        name: &str,
        call_name: &str,
        miss_args: u64,
        params: Vec<Param>,
        ret: Option<Rc<Type>>,
        attr: SyscallAttr,
    ) -> Self {
        Syscall {
            id,
            nr,
            miss_args,
            attr,
            ret,
            name: to_box_str(name),
            call_name: to_box_str(call_name),
            params: Vec::into_boxed_slice(params),
        }
    }
}

fn to_box_str<T: AsRef<str>>(s: T) -> Box<str> {
    let t = s.as_ref();
    String::into_boxed_str(t.to_string())
}

impl fmt::Display for Syscall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.attr != SyscallAttr::default() {
            writeln!(f, "{}", self.attr)?;
        }
        write!(f, "{}(", self.call_name)?;
        for (i, p) in self.params.iter().enumerate() {
            write!(f, "{}", p)?;
            if i != self.params.len() - 1 {
                write!(f, ",")?;
            }
        }
        if let Some(ref ret) = self.ret {
            write!(f, ") -> {}", ret)?;
        }
        Ok(())
    }
}
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub struct Param {
    /// Name of Field.
    pub name: Box<str>,
    /// Typeid of Field.
    pub ty: Rc<Type>,
    pub dir: Option<Dir>,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(dir) = self.dir {
            write!(f, " {}", dir)?;
        }
        write!(f, " {}", self.ty)
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy)]
pub enum Dir {
    In,
    Out,
    InOut,
}

impl fmt::Display for Dir {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let p = match self {
            Self::In => "In",
            Self::Out => "Out",
            Self::InOut => "InOut",
        };
        write!(f, "{}", p)
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy)]
pub enum BinFmt {
    Native,
    BigEndian,
    StrDec,
    StrHex,
    StrOct,
}

impl fmt::Display for BinFmt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let b = match self {
            Self::Native => "Native",
            Self::BigEndian => "BigEndian",
            Self::StrDec => "StrDec",
            Self::StrHex => "StrHex",
            Self::StrOct => "StrOct",
        };
        write!(f, "{}", b)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SyscallAttr {
    disable: bool,
    timeout: u64,
    prog_tmout: u64,
    ignore_ret: bool,
    brk_ret: bool,
}

impl Default for SyscallAttr {
    fn default() -> Self {
        Self {
            disable: false,
            timeout: 0,
            prog_tmout: 0,
            ignore_ret: true,
            brk_ret: false,
        }
    }
}

impl fmt::Display for SyscallAttr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self == &SyscallAttr::default() {
            return Ok(());
        }
        write!(f, "#[")?;
        if self.disable {
            write!(f, "disable,")?;
        }
        if self.ignore_ret {
            write!(f, "ignore_ret,")?;
        }
        if self.brk_ret {
            write!(f, "brk_ret,")?;
        }
        if self.timeout != 0 {
            write!(f, "timeout={},", self.timeout)?;
        }
        if self.prog_tmout != 0 {
            write!(f, "prog_tmout={},", self.prog_tmout)?;
        }
        Ok(())
    }
}

pub type TypeId = usize;

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub struct Type {
    id: TypeId,
    name: Box<str>,
    sz: usize,
    align: usize,
    optional: bool,
    varlen: bool,
    kind: TypeKind,
}

impl Type {
    pub fn new(
        id: TypeId,
        name: &str,
        sz: usize,
        align: usize,
        optional: bool,
        varlen: bool,
        kind: TypeKind,
    ) -> Self {
        Self {
            id,
            sz,
            align,
            optional,
            varlen,
            kind,
            name: to_box_str(name),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub enum TypeRef {
    Id(TypeId),
    Ref(Rc<Type>),
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub enum TypeKind {
    Res {
        fmt: BinFmt,
        desc: ResDesc,
    },
    Const {
        int_fmt: IntFmt,
        val: u64,
        pad: bool,
    },
    Int {
        int_fmt: IntFmt,
        range: Option<(u64, u64)>,
        align: u64,
    },
    Flags {
        int_fmt: IntFmt,
        vals: Box<[u64]>,
        bitmask: bool,
    },
    Len {
        int_fmt: IntFmt,
        bit_sz: u64,
        offset: bool,
        path: Box<[Box<str>]>,
    },
    Proc {
        int_fmt: IntFmt,
        start: u64,
        per_proc: u64,
    },
    Csum {
        int_fmt: IntFmt,
        kind: CsumKind,
        buf: Option<Box<str>>,
        protocol: u64,
    },
    Vma {
        begin: u64,
        end: u64,
    },
    Buffer {
        kind: BufferKind,
        subkind: Option<Box<str>>,
    },
    Array {
        range: Option<(u64, u64)>,
        elem: TypeRef,
    },
    Ptr {
        elem: TypeRef,
        dir: Dir,
    },
    Struct {
        fields: Box<[Field]>,
        align_attr: u64,
    },
    Union {
        fields: Box<[Field]>,
    },
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub struct Field {
    /// Name of Field.
    pub name: Box<str>,
    /// Typeid of Field.
    pub ty: TypeRef,
    pub dir: Option<Dir>,
}

impl TypeKind {
    pub fn new_buffer(kind: BufferKind, subkind: &str) -> Self {
        todo!()
    }
    pub fn new_struct(align: u64, fields: Vec<Field>) -> Self {
        todo!()
    }

    pub fn new_union(fields: Vec<Field>) -> Self {
        todo!()
    }

    pub fn new_len(int_fmt: IntFmt, bit_sz: u64, offset: bool, path: Vec<&str>) -> Self {
        todo!()
    }

    pub fn new_csum(int_fmt: IntFmt, kind: CsumKind, buf: Option<&str>, proto: u64) -> Self {
        todo!()
    }

    pub fn new_flags(intfmt: IntFmt, vals: Vec<u64>, bitmask: bool) -> Self {
        todo!()
    }

    pub fn void() -> Self {
        Self::Buffer {
            kind: BufferKind::BlobRange(0, 0),
            subkind: None,
        }
    }
}
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub struct ResDesc {
    /// Name of resource.
    name: Box<str>,
    /// Subkind of these kind resource.
    kinds: Box<[Box<str>]>,
    /// Special value for current resource.
    vals: Box<[u64]>,
    // /// Possible constructor
    // ctors: Box<[(SyscallId, bool)]>,
}

impl ResDesc {
    pub fn new(name: &str, kinds: Vec<&str>, vals: Vec<u64>) -> Self {
        todo!()
    }
}
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]

pub struct IntFmt {
    fmt: BinFmt,
    bitfield_off: u64,
    bitfield_len: u64,
    bitfield_unit: u64,
    bitfield_unit_off: u64,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub enum CsumKind {
    Inet,
    Pseudo,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub enum BufferKind {
    BlobRand,
    BlobRange(u64, u64),
    String { vals: Box<[Box<[u8]>]>, noz: bool },
    Filename { vals: Box<Box<str>>, noz: bool },
    Text(TextKind),
}

impl BufferKind {
    pub fn new_str(vals: Vec<&[u8]>, noz: bool) -> Self {
        todo!()
    }
    pub fn new_fname(vals: Vec<&str>, noz: bool) -> Self {
        todo!()
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]
pub enum TextKind {
    Target,
    X86Real,
    X86bit16,
    X86bit32,
    X86bit64,
    Arm64,
}
