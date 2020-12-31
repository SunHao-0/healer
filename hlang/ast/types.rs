//! Abstract representation or AST type information.
use super::{to_boxed_str, Dir, Syscall};
use rustc_hash::FxHashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub type TypeId = usize;

#[derive(Debug)]
pub struct Type {
    pub id: TypeId,
    pub name: Box<str>,
    pub sz: u64,
    pub align: u64,
    pub optional: bool,
    pub varlen: bool,
    pub kind: TypeKind,
}

impl Type {
    pub fn new(
        id: TypeId,
        name: &str,
        sz: u64,
        align: u64,
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
            name: to_boxed_str(name),
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

    pub fn get_buffer_kind(&self) -> Option<&BufferKind> {
        match &self.kind {
            TypeKind::Buffer { kind, .. } => Some(kind),
            _ => None,
        }
    }

    pub fn get_ptr_info(&self) -> Option<(&Rc<Type>, Dir)> {
        match &self.kind {
            TypeKind::Ptr { elem, dir } => Some((elem.as_ref().unwrap(), *dir)),
            _ => None,
        }
    }

    #[allow(clippy::type_complexity)]
    pub fn get_array_info(&self) -> Option<(&Rc<Type>, Option<(u64, u64)>)> {
        match &self.kind {
            TypeKind::Array { elem, range } => Some((elem.as_ref().unwrap(), *range)),
            _ => None,
        }
    }

    pub fn get_fields(&self) -> Option<&[Field]> {
        match &self.kind {
            TypeKind::Struct { fields, .. } | TypeKind::Union { fields } => Some(fields),
            _ => None,
        }
    }

    pub fn get_len_info(&self) -> Option<Rc<LenInfo>> {
        match &self.kind {
            TypeKind::Len { len_info, .. } => Some(Rc::clone(len_info)),
            _ => None,
        }
    }

    pub fn get_template_name(&self) -> &str {
        let name = &*self.name;
        if let Some(idx) = name.find('[') {
            &name[0..idx]
        } else {
            name
        }
    }

    pub fn get_int_fmt(&self) -> Option<&IntFmt> {
        match &self.kind {
            TypeKind::Int { int_fmt, .. }
            | TypeKind::Flags { int_fmt, .. }
            | TypeKind::Csum { int_fmt, .. }
            | TypeKind::Proc { int_fmt, .. }
            | TypeKind::Const { int_fmt, .. }
            | TypeKind::Len { int_fmt, .. } => Some(int_fmt),
            _ => None,
        }
    }

    pub fn bf_offset(&self) -> u64 {
        if let Some(int_fmt) = self.get_int_fmt() {
            int_fmt.bitfield_off
        } else {
            0
        }
    }

    pub fn bf_len(&self) -> u64 {
        if let Some(int_fmt) = self.get_int_fmt() {
            int_fmt.bitfield_len
        } else {
            0
        }
    }

    pub fn bin_fmt(&self) -> BinFmt {
        match &self.kind {
            TypeKind::Int { int_fmt, .. }
            | TypeKind::Flags { int_fmt, .. }
            | TypeKind::Csum { int_fmt, .. }
            | TypeKind::Proc { int_fmt, .. }
            | TypeKind::Const { int_fmt, .. }
            | TypeKind::Len { int_fmt, .. } => int_fmt.fmt,
            TypeKind::Res { fmt, .. } => *fmt,
            _ => BinFmt::Native,
        }
    }

    pub fn unit_offset(&self) -> u64 {
        if let Some(int_fmt) = self.get_int_fmt() {
            int_fmt.bitfield_unit_off
        } else {
            0
        }
    }

    pub fn is_bitfield(&self) -> bool {
        if let Some(int_fmt) = self.get_int_fmt() {
            int_fmt.bitfield_len != 0
        } else {
            false
        }
    }

    pub fn is_readable_date_type(&self) -> bool {
        match &self.kind {
            TypeKind::Buffer { kind, .. } => {
                matches!(kind, BufferKind::String { .. } | BufferKind::Filename { .. })
            }
            _ => false,
        }
    }

    pub fn is_pad(&self) -> bool {
        if let TypeKind::Const { pad, .. } = &self.kind {
            *pad
        } else {
            false
        }
    }
}

/// Order by TypeId
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

/// Compare by TypeId
impl PartialEq for Type {
    fn eq(&self, other: &Type) -> bool {
        self.id == other.id
    }
}

impl Eq for Type {}

/// Hash by TypeId
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

/// Ref of another type.
/// Id ref is only used during constructing target.
#[derive(Eq, PartialEq, Debug, Clone)]
pub enum TypeRef {
    /// Temporary ref representation for convenience.
    Id(TypeId),
    /// Share ref.
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

/// Type kind.
#[derive(Eq, PartialEq, Debug, Clone)]
pub enum TypeKind {
    /// Resource kind.
    Res {
        fmt: BinFmt,
        desc: ResDesc,
    },
    /// Const kind.
    Const {
        int_fmt: IntFmt,
        val: u64,
        pad: bool,
    },
    /// Integer kind.
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
        len_info: Rc<LenInfo>,
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
    /// Direction of field.
    pub dir: Option<Dir>,
}

impl TypeKind {
    pub fn new_buffer(kind: BufferKind, subkind: &str) -> Self {
        TypeKind::Buffer {
            kind,
            subkind: if subkind.is_empty() {
                None
            } else {
                Some(to_boxed_str(subkind))
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
        let path = path.iter().map(to_boxed_str).collect::<Vec<_>>();
        TypeKind::Len {
            int_fmt,
            len_info: Rc::new(LenInfo {
                bit_sz,
                offset,
                path: path.into_boxed_slice(),
            }),
        }
    }

    pub fn new_csum(int_fmt: IntFmt, kind: CsumKind, buf: Option<&str>, proto: u64) -> Self {
        TypeKind::Csum {
            int_fmt,
            kind,
            buf: buf.map(to_boxed_str),
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

    pub fn is_str_like(&self) -> bool {
        if let TypeKind::Buffer { kind, .. } = self {
            matches!(kind, BufferKind::Filename { .. } | BufferKind::String { .. })
        } else {
            false
        }
    }
}
#[derive(Eq, PartialEq, Debug, Clone)]
pub struct LenInfo {
    pub bit_sz: u64,
    pub offset: bool,
    pub path: Box<[Box<str>]>,
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
    /// Possible constructors.
    pub ctors: FxHashSet<Rc<Syscall>>,
    /// Possible consumers.
    pub consumers: FxHashSet<Rc<Syscall>>,
}

impl ResDesc {
    pub fn new(name: &str, kinds: Vec<&str>, vals: Vec<u64>) -> Self {
        ResDesc {
            name: to_boxed_str(name),
            kinds: Vec::into_boxed_slice(kinds.iter().map(to_boxed_str).collect()),
            vals: vals.into_boxed_slice(),
            ty: None,
            ctors: FxHashSet::default(),
            consumers: FxHashSet::default(),
        }
    }
}

/// Binary format.
#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy)]
pub enum BinFmt {
    Native = 0,
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

/// Integer format.
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
    Filename { vals: Box<[Box<[u8]>]>, noz: bool },
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
        let vals = vals
            .iter()
            .map(|s| Vec::from(s.as_bytes()).into_boxed_slice())
            .collect::<Vec<_>>();
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
    Ppc64,
}
