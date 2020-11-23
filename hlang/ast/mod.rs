//! Abstract representation or AST of system call model.

pub mod types;
pub mod value;
pub use types::*;
pub use value::*;

use rustc_hash::FxHashMap;
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
            name: to_boxed_str(name),
            call_name: to_boxed_str(call_name),
            params: Vec::into_boxed_slice(params),
            input_res: FxHashMap::default(),
            output_res: FxHashMap::default(),
        }
    }
}

pub(crate) fn to_boxed_str<T: AsRef<str>>(s: T) -> Box<str> {
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
            name: to_boxed_str(name),
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

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Clone, Copy, Hash)]
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
    pub calls: Vec<Call>, // may be add other analysis data
}

impl Prog {
    pub fn new(calls: Vec<Call>) -> Self {
        Prog { calls }
    }
}

pub struct Call {
    pub meta: Rc<Syscall>,
    pub args: Vec<Value>,
    pub ret: Option<Rc<ResValue>>,
}

impl Call {
    pub fn new(meta: Rc<Syscall>, args: Vec<Value>, ret: Option<Rc<ResValue>>) -> Self {
        Self { meta, args, ret }
    }
}
