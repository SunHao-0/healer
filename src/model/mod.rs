//! Abstract representation or AST of system call model.

use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

pub mod types;
pub mod value;
pub use types::*;
pub use value::*;
/// System call id.
pub type SId = usize;

pub type SyscallRef = &'static Syscall;

/// Information related to particular system call.
#[derive(Debug, Clone)]
pub struct Syscall {
    /// Index of system call, diffierent from nr.
    pub id: SId,
    /// Call number, set to 0 for system that doesn't use nr.
    pub nr: u64,
    /// Name of specialized call.
    pub name: String,
    /// Name of system call.
    pub call_name: String,
    /// Syzkaller: Number of trailing args that should be zero-filled.
    pub missing_args: u64,
    /// Parameters of calls.
    pub params: Vec<Param>,
    /// Return type of system call: a ref to res type or None.
    pub ret: Option<TypeRef>,
    /// Attributes of system call.
    pub attr: SyscallAttr,
    /// Resources consumed by current system call.
    pub input_res: FxHashSet<TypeRef>,
    /// Resource produced by current system call.
    pub output_res: FxHashSet<TypeRef>,
}

impl fmt::Display for Syscall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let attr = format!("{}", self.attr);
        if !attr.is_empty() {
            writeln!(f, "{}", attr)?;
        }
        write!(f, "fn {}(", self.name)?;
        for (i, p) in self.params.iter().enumerate() {
            write!(f, "{}", p)?;
            if i != self.params.len() - 1 {
                write!(f, ",")?;
            }
        }
        write!(f, ")")?;
        if let Some(ret) = self.ret {
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
    pub name: String,
    /// Typeid of Field.
    pub ty: TypeRef,
    pub dir: Option<Dir>,
}

impl Param {
    pub fn new(name: String, ty: TypeRef, dir: Option<Dir>) -> Self {
        Self { name, ty, dir }
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

pub struct Prog {
    pub calls: Vec<Call>,
}

struct CloneCtx {
    res_addr: FxHashMap<*const ResValue, *mut ResValue>,
}

impl Clone for Prog {
    fn clone(&self) -> Self {
        let mut ctx = CloneCtx {
            res_addr: FxHashMap::default(),
        };
        let mut calls = Vec::with_capacity(self.calls.len());
        for c in self.calls.iter() {
            calls.push(clone_call(&mut ctx, c))
        }
        Prog { calls }
    }
}

fn clone_call(ctx: &mut CloneCtx, c: &Call) -> Call {
    let mut args = Vec::with_capacity(c.args.len());
    for arg in c.args.iter() {
        args.push(clone_value(ctx, arg));
    }
    let mut ret = None;
    if let Some(call_ret) = c.ret.as_ref() {
        ret = Some(clone_value(ctx, call_ret));
    }
    Call {
        meta: c.meta,
        args,
        ret,
        // val_cnt: c.val_cnt,
        // res_cnt: c.res_cnt,
    }
}

fn clone_value(ctx: &mut CloneCtx, v: &Value) -> Value {
    match &v.kind {
        ValueKind::Scalar(val) => Value::new(v.dir, v.ty, ValueKind::new_scalar(*val)),
        ValueKind::Ptr { addr, pointee } => {
            if let Some(p) = pointee.as_ref() {
                let pointee = clone_value(ctx, p);
                Value::new(v.dir, v.ty, ValueKind::new_ptr(*addr, pointee))
            } else {
                Value::new(v.dir, v.ty, ValueKind::new_ptr_null())
            }
        }
        ValueKind::Vma { addr, size } => Value::new(v.dir, v.ty, ValueKind::new_vma(*addr, *size)),
        ValueKind::Bytes { val, sz } => {
            if val.is_empty() {
                Value::new(v.dir, v.ty, ValueKind::new_out_bytes(*sz))
            } else {
                Value::new(v.dir, v.ty, ValueKind::new_in_bytes(val.clone()))
            }
        }
        ValueKind::Group(vals) => {
            let mut vals_new = Vec::with_capacity(vals.len());
            for v in vals.iter() {
                vals_new.push(clone_value(ctx, v));
            }
            Value::new(v.dir, v.ty, ValueKind::new_group(vals_new))
        }
        ValueKind::Union { idx, val } => {
            let val_new = clone_value(ctx, val);
            Value::new(v.dir, v.ty, ValueKind::new_union(*idx, val_new))
        }
        ValueKind::Res(val) => {
            if let Some(id) = val.kind.id() {
                let mut val_new = Box::new(ResValue::new_res(val.val, id));
                ctx.res_addr
                    .insert(&**val as *const ResValue, &mut *val_new as *mut ResValue);
                Value::new(v.dir, v.ty, ValueKind::new_res(val_new))
            } else if let Some(src) = val.kind.src() {
                let src_new = ctx.res_addr[&(src as *const _)];
                Value::new(v.dir, v.ty, unsafe {
                    ValueKind::new_res_ref(src_new.as_mut().unwrap())
                })
            } else {
                Value::new(v.dir, v.ty, ValueKind::new_res_null(val.val))
            }
        }
    }
}

impl fmt::Display for Prog {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for call in self.calls.iter() {
            writeln!(f, "{}", call)?
        }
        Ok(())
    }
}

impl Prog {
    pub fn new(calls: Vec<Call>) -> Self {
        Prog { calls }
    }

    pub fn remove(&self, i: usize) -> Prog {
        let mut p = self.clone();
        p.calls.remove(i);
        p
    }

    pub fn sub_prog(&self, n: usize) -> Prog {
        let mut ctx = CloneCtx {
            res_addr: FxHashMap::default(),
        };
        let mut calls = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let call = clone_call(&mut ctx, &self.calls[i]);
            calls.push(call);
        }
        Prog { calls }
    }
}

#[derive(Clone)]
pub struct ProgWrapper(pub Prog);

impl ProgWrapper {
    pub fn to_prog(&self) -> Prog {
        self.0.clone()
    }
}

unsafe impl Send for ProgWrapper {}

pub struct Call {
    pub meta: SyscallRef,
    pub args: Vec<Value>,
    pub ret: Option<Value>,
}

impl fmt::Display for Call {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ret) = &self.ret {
            let id = ret.res_id().unwrap();
            write!(f, "r{} = ", id)?;
        }
        write!(f, "{}(", self.meta.name)?;
        for (i, arg) in self.args.iter().enumerate() {
            write!(f, "{}", arg)?;
            if i != self.args.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}
