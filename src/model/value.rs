//! Abstract representation of value structure of different types.
use crate::model::types::TypeRef;
use crate::model::Dir;

use std::fmt;

use rustc_hash::FxHashSet;

#[derive(Debug)]
pub struct Value {
    /// Direction of value.
    pub dir: Dir,
    /// Original type of value.
    pub ty: TypeRef,
    /// Actual value storage.
    pub kind: ValueKind,
}

impl Value {
    pub fn new(dir: Dir, ty: TypeRef, kind: ValueKind) -> Self {
        Self { dir, ty, kind }
    }

    pub fn inner_val(&self) -> Option<&Value> {
        if let ValueKind::Ptr { ref pointee, .. } = self.kind {
            if let Some(pointee) = pointee {
                Self::inner_val(pointee)
            } else {
                None
            }
        } else {
            Some(self)
        }
    }

    pub fn inner_val_mut(&mut self) -> Option<&mut Value> {
        if let ValueKind::Ptr {
            ref mut pointee, ..
        } = self.kind
        {
            if let Some(pointee) = pointee {
                Self::inner_val_mut(pointee)
            } else {
                None
            }
        } else {
            Some(self)
        }
    }

    pub fn scalar_val(&self) -> (u64, u64) {
        use super::types::TypeKind;

        match &self.ty.kind {
            TypeKind::Int { .. }
            | TypeKind::Flags { .. }
            | TypeKind::Csum { .. }
            | TypeKind::Res { .. }
            | TypeKind::Const { .. }
            | TypeKind::Len { .. } => (self.kind.scalar_val().unwrap(), 0),
            TypeKind::Proc {
                start, per_proc, ..
            } => (*start + self.kind.scalar_val().unwrap(), *per_proc),
            _ => unreachable!(),
        }
    }

    pub fn unit_sz(&self) -> u64 {
        if let Some(int_fmt) = self.ty.int_fmt() {
            if int_fmt.bitfield_len != 0 {
                return int_fmt.bitfield_unit;
            }
        }
        self.size()
    }

    pub fn vma_size(&self) -> Option<u64> {
        if let ValueKind::Vma { size, .. } = &self.kind {
            Some(*size)
        } else {
            None
        }
    }

    pub fn bytes_val(&self) -> Option<&[u8]> {
        if let ValueKind::Bytes(v) = &self.kind {
            Some(v)
        } else {
            None
        }
    }

    pub fn group_val(&self) -> Option<&[Value]> {
        if let ValueKind::Group(vals) = &self.kind {
            Some(vals)
        } else {
            None
        }
    }

    pub fn group_val_mut(&mut self) -> Option<&mut [Value]> {
        if let ValueKind::Group(vals) = &mut self.kind {
            Some(vals)
        } else {
            None
        }
    }

    pub fn res_id(&self) -> Option<usize> {
        match &self.kind {
            ValueKind::Res(e) => e.res_id(),
            _ => None,
        }
    }

    pub fn res_rc(&self) -> Option<usize> {
        match &self.kind {
            ValueKind::Res(e) => e.res_rc(),
            _ => None,
        }
    }

    pub fn res_val(&self) -> Option<&ResValue> {
        match &self.kind {
            ValueKind::Res(e) => Some(e),
            _ => None,
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use super::types::TypeKind;
        match &self.kind {
            ValueKind::Scalar(val) => match &self.ty.kind {
                TypeKind::Const { .. } => write!(f, "{:#X}(const)", *val),
                TypeKind::Csum { .. } => write!(f, "{:#X}(csum)", *val),
                TypeKind::Len { len_info, .. } => {
                    let mut len_name = "len";
                    if len_info.offset {
                        len_name = "offset";
                    }
                    if len_info.bit_sz != 0 {
                        len_name = "bitsize"
                    }
                    write!(f, "{:#X}({})", val, len_name)
                }
                _ => write!(f, "{:#X}", *val),
            },
            ValueKind::Ptr { addr, pointee } => {
                if let Some(ref pointee) = pointee {
                    write!(f, "&{:#X}={}", *addr, pointee)
                } else {
                    write!(f, "NULL")
                }
            }
            ValueKind::Vma { addr, size } => write!(f, "vma=({}, {})", *addr, *size),
            ValueKind::Bytes(val) => {
                if self.ty.is_str_like() {
                    write!(f, "\"{}\"", String::from_utf8_lossy(val))
                } else {
                    write!(f, "{:X?}", val)
                }
            }
            ValueKind::Group(vals) => {
                let mut open_brackets = '[';
                let mut close_brackets = ']';
                let mut ty = "array";
                if let TypeKind::Struct { .. } = &self.ty.kind {
                    open_brackets = '{';
                    close_brackets = '}';
                    ty = "struct";
                }
                write!(f, "{}{}", ty, open_brackets)?;
                for (id, val) in vals.iter().enumerate() {
                    write!(f, "{}", val)?;
                    if id != vals.len() - 1 {
                        write!(f, ",")?;
                    }
                }
                write!(f, "{}", close_brackets)
            }
            ValueKind::Union { val, idx } => write!(
                f,
                "union{{{}: {}}}",
                self.ty.fields().unwrap()[*idx].name,
                val
            ),
            ValueKind::Res(r) => write!(f, "{}", r),
        }
    }
}

#[derive(Debug)]
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
    Res(Box<ResValue>),
}

impl ValueKind {
    pub fn new_scalar(val: u64) -> Self {
        ValueKind::Scalar(val)
    }

    pub fn new_ptr(addr: u64, pointee: Option<Value>) -> Self {
        ValueKind::Ptr {
            addr,
            pointee: pointee.map(Box::new),
        }
    }

    pub fn new_ptr_null() -> Self {
        ValueKind::new_ptr(0, None)
    }

    pub fn new_vma(addr: u64, size: u64) -> Self {
        ValueKind::Vma { addr, size }
    }

    pub fn new_bytes<T: Into<Box<[u8]>>>(vals: T) -> Self {
        ValueKind::Bytes(vals.into())
    }

    pub fn new_group(vals: Vec<Value>) -> Self {
        ValueKind::Group(vals)
    }

    pub fn new_union(idx: usize, val: Value) -> Self {
        ValueKind::Union {
            idx,
            val: Box::new(val),
        }
    }

    pub(crate) fn new_res_ref(src: *mut ResValue) -> Self {
        let mut res_val = Box::new(ResValue::new_res_ref(src));
        unsafe { (*src).kind.add_ref(&mut *res_val as *mut ResValue) }
        ValueKind::Res(res_val)
    }

    pub fn new_res(kind: Box<ResValue>) -> Self {
        ValueKind::Res(kind)
    }

    pub fn new_res_null(val: u64) -> Self {
        ValueKind::Res(Box::new(ResValue::new_null(val)))
    }

    pub fn scalar_val(&self) -> Option<u64> {
        if let ValueKind::Scalar(val) = self {
            Some(*val)
        } else if let ValueKind::Res(res) = &self {
            Some(res.val)
        } else {
            None
        }
    }
}

/// Value of resource type.
#[derive(Debug)]
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

    pub fn new_res_ref(src: *mut ResValue) -> Self {
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

    pub fn res_id(&self) -> Option<usize> {
        self.kind.id()
    }

    pub fn res_rc(&self) -> Option<usize> {
        self.kind.rc()
    }
}

impl fmt::Display for ResValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.kind {
            ResValueKind::Own { id, .. } => write!(f, "r{}(out)", *id),
            ResValueKind::Ref { src } => write!(
                f,
                "r{}",
                unsafe { (*src).as_ref().unwrap() }.res_id().unwrap()
            ),
            ResValueKind::Null => write!(f, "{:#X}(null)", self.val),
        }
    }
}

impl Drop for ResValue {
    fn drop(&mut self) {
        match &self.kind {
            ResValueKind::Own { refs, .. } => {
                for r in refs.iter().copied() {
                    unsafe { (*r).kind = ResValueKind::Null };
                }
            }
            ResValueKind::Ref { src } => {
                unsafe { (**src).kind.remove_ref(self as *mut ResValue) };
            }
            ResValueKind::Null => {}
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
        /// ResValue that ref current resource.
        /// Ensure the validity of the address manually.
        refs: FxHashSet<*mut ResValue>,
    },
    /// Ref some other resources outputed by previous calls.
    /// Ensure the validity of the address manually.
    Ref { src: *mut ResValue },
    /// Do not own or ref any resource, only contains special value.
    Null,
}

impl ResValueKind {
    pub fn remove_ref(&mut self, r: *mut ResValue) {
        if let ResValueKind::Own { refs, .. } = self {
            refs.remove(&r);
        } else {
            unreachable!()
        }
    }

    pub fn add_ref(&mut self, r: *mut ResValue) {
        if let ResValueKind::Own { refs, .. } = self {
            refs.insert(r);
        } else {
            unreachable!()
        }
    }

    pub fn new_ref_kind(src: *mut ResValue) -> Self {
        // make sure add current ResValue  to `src`'s refs
        Self::Ref { src }
    }

    pub fn new_res_kind(id: usize) -> Self {
        Self::Own {
            id,
            refs: FxHashSet::default(),
        }
    }

    pub fn id(&self) -> Option<usize> {
        if let ResValueKind::Own { id, .. } = self {
            Some(*id)
        } else {
            None
        }
    }

    pub fn rc(&self) -> Option<usize> {
        if let ResValueKind::Own { refs, .. } = self {
            Some(refs.len())
        } else {
            None
        }
    }

    pub fn src(&self) -> Option<*mut ResValue> {
        if let ResValueKind::Ref { src } = self {
            Some(*src)
        } else {
            None
        }
    }
}
