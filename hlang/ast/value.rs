//! Abstract representation of value structure of different types.
use super::types::Type;
use super::Dir;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Value {
    /// Direction of value.
    pub dir: Dir,
    /// Original type of value.
    pub ty: Rc<Type>,
    /// Actual value storage.
    pub kind: ValueKind,
}

impl Value {
    pub fn new(dir: Dir, ty: Rc<Type>, kind: ValueKind) -> Self {
        Self { dir, ty, kind }
    }

    pub fn new_scalar(dir: Dir, ty: Rc<Type>, val: u64) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Scalar(val),
        }
    }

    pub fn new_ptr(dir: Dir, ty: Rc<Type>, addr: u64, pointee: Option<Value>) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Ptr {
                addr,
                pointee: pointee.map(Box::new),
            },
        }
    }

    pub fn new_vma(dir: Dir, ty: Rc<Type>, addr: u64, size: u64) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Vma { addr, size },
        }
    }

    pub fn new_bytes<T: Into<Box<[u8]>>>(dir: Dir, ty: Rc<Type>, vals: T) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Bytes(vals.into()),
        }
    }

    pub fn new_group(dir: Dir, ty: Rc<Type>, vals: Vec<Value>) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Group(vals),
        }
    }

    pub fn new_union(dir: Dir, ty: Rc<Type>, idx: usize, val: Value) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Union {
                idx,
                val: Box::new(val),
            },
        }
    }

    pub fn new_res_ref(dir: Dir, ty: Rc<Type>, src: Rc<ResValue>) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Res(Rc::new(ResValue::new_ref_res(src))),
        }
    }

    pub fn new_res(dir: Dir, ty: Rc<Type>, kind: Rc<ResValue>) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Res(kind),
        }
    }

    pub fn new_res_null(dir: Dir, ty: Rc<Type>, val: u64) -> Self {
        Self {
            dir,
            ty,
            kind: ValueKind::Res(Rc::new(ResValue::new_null(val))),
        }
    }

    pub fn size(&self) -> u64 {
        use super::types::TypeKind;
        match &self.kind {
            ValueKind::Scalar { .. }
            | ValueKind::Ptr { .. }
            | ValueKind::Vma { .. }
            | ValueKind::Res(_) => self.ty.sz,
            ValueKind::Bytes(bytes) => bytes.len() as u64,
            ValueKind::Group(vals) => {
                if !self.ty.varlen {
                    self.ty.sz
                } else {
                    match &self.ty.kind {
                        TypeKind::Struct { align_attr, .. } => {
                            let mut sz = 0;
                            for val in vals {
                                sz += val.size();
                            }
                            if *align_attr != 0 && sz % *align_attr != 0 {
                                sz += *align_attr - sz % *align_attr;
                            }
                            sz
                        }
                        TypeKind::Array { .. } => {
                            let mut sz = 0;
                            for val in vals {
                                sz += val.size();
                            }
                            sz
                        }
                        _ => unreachable!(),
                    }
                }
            }
            ValueKind::Union { val, .. } => {
                if !self.ty.varlen {
                    self.ty.sz
                } else {
                    val.size()
                }
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        todo!()
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum ValueKind {
    /// For integer, len, csum, proc, flag type, store its scarlar value.
    Scalar(u64),
    /// For ptr type, store its relative address and underlining value.
    Ptr {
        addr: u64,
        pointee: Option<Box<Value>>,
    },
    /// For vma type, store its relative address and page number.
    Vma { addr: u64, size: u64 },
    /// For buffer type, store its actual bytes value.
    Bytes(Box<[u8]>),
    /// For struct and array type, store a slice of value.
    Group(Vec<Value>),
    /// For union, store its value and index of selected field.
    Union { idx: usize, val: Box<Value> },
    /// For resource type, store a flag to indicate whether it is
    /// expected to output resource value or ref previous generated resource value.
    Res(Rc<ResValue>),
}

impl ValueKind {
    pub fn get_scalar_val(&self) -> Option<u64> {
        if let ValueKind::Scalar(val) = self {
            Some(*val)
        } else {
            None
        }
    }

    pub fn get_bytes_val(&self) -> Option<&[u8]> {
        if let ValueKind::Bytes(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn get_res_id(&self) -> Option<usize> {
        match &self {
            ValueKind::Res(e) => e.get_res_id(),
            _ => None,
        }
    }
}

/// Value of resource type.
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct ResValue {
    pub val: u64,
    pub op_add: u64,
    pub op_div: u64,
    pub kind: ResValueKind,
}

impl ResValue {
    pub fn new_res(val: u64, id: usize) -> Self {
        Self {
            op_add: 0,
            op_div: 0,
            kind: ResValueKind::new_res_kind(id),
            val,
        }
    }

    pub fn new_ref_res(src: Rc<ResValue>) -> Self {
        Self {
            val: 0,
            op_add: 0,
            op_div: 0,
            kind: ResValueKind::new_ref_kind(src),
        }
    }

    pub fn new_null(val: u64) -> Self {
        Self {
            val,
            op_add: 0,
            op_div: 0,
            kind: ResValueKind::Null,
        }
    }

    pub fn inc_ref_count_uncheck(&self) {
        self.kind.inc_ref_count_uncheck();
    }

    pub fn get_res_id(&self) -> Option<usize> {
        if let ResValueKind::Own { id, .. } = &self.kind {
            Some(*id)
        } else {
            None
        }
    }
}

/// Resource value kind.
///
/// Resource value can not be generated by ourselves, it is output from some other syscalls.
/// Therefore, we can only mark current value to indicate that it ref some reources value
/// generated from previous calls, or it is expected to output some resources and refed by other calls.
#[derive(Debug)]
pub enum ResValueKind {
    /// Current syscall is expected to output this resource.
    Own {
        id: usize,
        refs: std::cell::Cell<usize>,
    },
    /// Current syscall ref some other resources outputed by previous calls.
    Ref { src: Rc<ResValue> },
    /// Do not own or ref any resource, only contains special value.
    Null,
}

impl ResValueKind {
    pub fn inc_ref_count_uncheck(&self) {
        if let ResValueKind::Own { refs, .. } = self {
            let count = refs.get() + 1;
            refs.set(count);
        } else {
            unreachable!()
        }
    }

    pub fn new_ref_kind(src: Rc<ResValue>) -> Self {
        src.inc_ref_count_uncheck();
        Self::Ref { src }
    }

    pub fn new_res_kind(id: usize) -> Self {
        Self::Own {
            id,
            refs: std::cell::Cell::new(0),
        }
    }

    pub fn get_id(&self) -> Option<usize> {
        if let ResValueKind::Own { id, .. } = self {
            Some(*id)
        } else {
            None
        }
    }

    pub fn get_src(&self) -> Option<&Rc<ResValue>> {
        if let ResValueKind::Ref { src } = self {
            Some(src)
        } else {
            None
        }
    }
}

impl Hash for ResValueKind {
    fn hash<H: Hasher>(&self, h: &mut H) {
        match self {
            ResValueKind::Own { id, .. } => {
                h.write_usize(*id);
                // encode Own itself.
                h.write_usize(0x2519567851);
            }
            ResValueKind::Ref { src } => {
                // encode Ref itself.
                h.write_usize(0x8855738149);
                src.hash(h);
            }
            ResValueKind::Null => {
                h.write_usize(0x47022874);
            }
        }
    }
}

impl PartialEq for ResValueKind {
    fn eq(&self, other: &ResValueKind) -> bool {
        if let Some(id0) = self.get_id() {
            if let Some(id1) = other.get_id() {
                id0 == id1
            } else {
                false
            }
        } else if let Some(src0) = self.get_src() {
            if let Some(src1) = other.get_src() {
                src0.eq(src1)
            } else {
                false
            }
        } else {
            true
        }
    }
}

impl Eq for ResValueKind {}
