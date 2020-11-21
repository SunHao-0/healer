//! Abstract representation or AST of system call model.
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// System call id.
pub type SId = usize;

/// Information related to particular system call.
#[derive(Debug, Clone)]
pub struct Syscall {
    /// Index of system call, diffierent from nr.
    pub id: SId,
    /// Call number, set to 0 for system that doesn't use nr.
    pub nr: u64,
    /// System call name.
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
    /// Resources consumed by current system call.
    /// Key is resourse type, value is count of that kind of resource .
    pub input_res: FxHashMap<Rc<Type>, usize>,
    /// Resource produced by current system call.
    /// Key is resourse type, value if count.
    pub output_res: FxHashMap<Rc<Type>, usize>,
}

impl Syscall {
    #[allow(clippy::too_many_arguments)]
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
            input_res: FxHashMap::default(),
            output_res: FxHashMap::default(),
        }
    }
}

fn to_box_str<T: AsRef<str>>(s: T) -> Box<str> {
    let t = s.as_ref();
    String::into_boxed_str(t.to_string())
}

impl fmt::Display for Syscall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let attr = format!("{}", self.attr);
        if !attr.is_empty() {
            writeln!(f, "{}", attr)?;
        }
        write!(f, "fn {}(", self.call_name)?;
        for (i, p) in self.params.iter().enumerate() {
            write!(f, "{}", p)?;
            if i != self.params.len() - 1 {
                write!(f, ",")?;
            }
        }
        write!(f, ")")?;
        if let Some(ref ret) = self.ret {
            write!(f, " -> {}", ret)?;
        }
        Ok(())
    }
}

impl PartialEq for Syscall {
    fn eq(&self, other: &Syscall) -> bool {
        self.id == other.id
    }
}

impl Eq for Syscall {}

impl Hash for Syscall {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.id)
    }
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Param {
    /// Name of Field.
    pub name: Box<str>,
    /// Typeid of Field.
    pub ty: Rc<Type>,
    pub dir: Option<Dir>,
}

impl Param {
    pub fn new(name: &str, ty: Rc<Type>, dir: Option<Dir>) -> Self {
        Self {
            name: to_box_str(name),
            ty,
            dir,
        }
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:", self.name)?;
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
    pub disable: bool,
    pub timeout: u64,
    pub prog_tmout: u64,
    pub ignore_ret: bool,
    pub brk_ret: bool,
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
        let mut buf = String::new();
        if self.disable {
            buf.push_str("disable,");
        }
        if self.ignore_ret {
            buf.push_str("ignore_ret,");
        }
        if self.brk_ret {
            buf.push_str("brk_ret,");
        }
        if self.timeout != 0 {
            buf.push_str(&format!("timeout={},", self.timeout));
        }
        if self.prog_tmout != 0 {
            buf.push_str(&format!("prog_tmout={},", self.prog_tmout));
        }
        if !buf.is_empty() {
            if buf.ends_with(',') {
                buf.pop();
            }
            write!(f, "#[{}]", buf)
        } else {
            Ok(())
        }
    }
}

pub type TypeId = usize;

#[derive(Debug, Clone)]
pub struct Type {
    pub id: TypeId,
    pub name: Box<str>,
    pub sz: usize,
    pub align: usize,
    pub optional: bool,
    pub varlen: bool,
    pub kind: TypeKind,
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

    pub fn is_res_kind(&self) -> bool {
        matches!(&self.kind, TypeKind::Res { .. })
    }

    pub fn res_desc(&self) -> Option<&ResDesc> {
        match &self.kind {
            TypeKind::Res { desc, .. } => Some(desc),
            _ => None,
        }
    }

    pub fn res_desc_mut(&mut self) -> Option<&mut ResDesc> {
        match &mut self.kind {
            TypeKind::Res { desc, .. } => Some(desc),
            _ => None,
        }
    }
}

impl PartialOrd for Type {
    fn partial_cmp(&self, other: &Type) -> Option<std::cmp::Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Ord for Type {
    fn cmp(&self, other: &Type) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Type) -> bool {
        self.id == other.id
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<T: Hasher>(&self, h: &mut T) {
        h.write_usize(self.id);
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub enum TypeRef {
    Id(TypeId),
    Ref(Rc<Type>),
}

impl TypeRef {
    pub fn as_id(&self) -> Option<TypeId> {
        match self {
            Self::Id(tid) => Some(*tid),
            Self::Ref(_) => None,
        }
    }

    pub fn as_ref(&self) -> Option<&Rc<Type>> {
        match &self {
            Self::Id(_) => None,
            Self::Ref(ty) => Some(ty),
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone)]
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

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Field {
    /// Name of Field.
    pub name: Box<str>,
    /// Typeid of Field.
    pub ty: TypeRef,
    pub dir: Option<Dir>,
}

impl TypeKind {
    pub fn new_buffer(kind: BufferKind, subkind: &str) -> Self {
        TypeKind::Buffer {
            kind,
            subkind: if subkind.is_empty() {
                None
            } else {
                Some(to_box_str(subkind))
            },
        }
    }
    pub fn new_struct(align: u64, fields: Vec<Field>) -> Self {
        TypeKind::Struct {
            fields: fields.into_boxed_slice(),
            align_attr: align,
        }
    }

    pub fn new_union(fields: Vec<Field>) -> Self {
        TypeKind::Union {
            fields: fields.into_boxed_slice(),
        }
    }

    pub fn new_len(int_fmt: IntFmt, bit_sz: u64, offset: bool, path: Vec<&str>) -> Self {
        let path = path.iter().map(to_box_str).collect::<Vec<_>>();
        TypeKind::Len {
            int_fmt,
            bit_sz,
            offset,
            path: path.into_boxed_slice(),
        }
    }

    pub fn new_csum(int_fmt: IntFmt, kind: CsumKind, buf: Option<&str>, proto: u64) -> Self {
        TypeKind::Csum {
            int_fmt,
            kind,
            buf: buf.map(to_box_str),
            protocol: proto,
        }
    }

    pub fn new_flags(int_fmt: IntFmt, vals: Vec<u64>, bitmask: bool) -> Self {
        TypeKind::Flags {
            int_fmt,
            vals: vals.into_boxed_slice(),
            bitmask,
        }
    }

    pub fn void() -> Self {
        Self::Buffer {
            kind: BufferKind::BlobRange(0, 0),
            subkind: None,
        }
    }
}
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct ResDesc {
    /// Name of resource.
    pub name: Box<str>,
    /// Subkind of these kind resource.
    pub kinds: Box<[Box<str>]>,
    /// Special value for current resource.
    pub vals: Box<[u64]>,
    /// Underlying type, None value for default u64.
    pub ty: Option<Rc<Type>>,
    /// Possible constructor
    pub ctors: FxHashSet<Rc<Syscall>>,
    /// Possible consumer
    pub consumers: FxHashSet<Rc<Syscall>>,
}

impl ResDesc {
    pub fn new(name: &str, kinds: Vec<&str>, vals: Vec<u64>) -> Self {
        ResDesc {
            name: to_box_str(name),
            kinds: Vec::into_boxed_slice(kinds.iter().map(to_box_str).collect()),
            vals: vals.into_boxed_slice(),
            ty: None,
            ctors: FxHashSet::default(),
            consumers: FxHashSet::default(),
        }
    }
}
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone)]

pub struct IntFmt {
    pub fmt: BinFmt,
    pub bitfield_off: u64,
    pub bitfield_len: u64,
    pub bitfield_unit: u64,
    pub bitfield_unit_off: u64,
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
    Filename { vals: Box<[Box<str>]>, noz: bool },
    Text(TextKind),
}

impl BufferKind {
    pub fn new_str(vals: Vec<&[u8]>, noz: bool) -> Self {
        let vals = vals
            .iter()
            .map(|&v| Vec::into_boxed_slice(Vec::from(v)))
            .collect::<Vec<_>>();
        BufferKind::String {
            vals: vals.into_boxed_slice(),
            noz,
        }
    }
    pub fn new_fname(vals: Vec<&str>, noz: bool) -> Self {
        let vals = vals.iter().map(to_box_str).collect::<Vec<_>>();
        BufferKind::Filename {
            vals: vals.into_boxed_slice(),
            noz,
        }
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

pub struct Prog(pub Vec<Call>);

pub struct Call {
    pub meta: Rc<Syscall>,
    pub args: Vec<Value>,
    pub ret: Value,
}

pub struct Value {
    pub dir: Dir,
    pub ty: Rc<Type>,
    pub kind: ValueKind,
}

pub enum ValueKind {
    Scalar(u64),
    Ptr { addr: u64, pointee: Box<Value> },
    Vma { addr: u64, size: u64 },
    Bytes(Box<[u8]>),
    Group(Vec<Value>),
    Ref,
}
