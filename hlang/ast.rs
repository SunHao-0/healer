#![allow(clippy::new_without_default)]
//! Abstract representation or AST of system call model.
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;
use std::{fmt, hash::Hash};

/// Information related to particular system call.
pub struct Syscall {
    /// Call number, set to 0 for system that doesn't use nr.
    pub nr: u32,
    /// System call name.
    // TODO This could be optimized. Not every syscall need store the whole name.
    pub name: Box<str>,
    /// Name of specialized part.
    /// For example, sp_name of open@special_file is special_file
    pub sp_name: Box<str>,
    /// Parameters of calls.
    pub params: Box<[Param]>,
    /// Return type of system call: a ref to res type or None.
    pub ret: Option<TypeId>,
    /// Attributes of system call.
    pub attrs: SyscallAttr,

    /// Input result parameters.
    input_res: FxHashSet<TypeId>,
    /// Output result parameters.
    output_res: FxHashSet<TypeId>,
    /// Name to param map.
    param_map: FxHashMap<Box<str>, usize>,
    /// Ref to types
    types: Rc<FxHashMap<TypeId, Type>>,
    /// Hash of current entry, decided by nr and sp_name.
    /// Must be inited properly.
    hash: u32,
}

impl PartialEq for Syscall {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.nr == other.nr && self.sp_name.eq(&other.sp_name)
    }
}

impl Eq for Syscall {}

impl Hash for Syscall {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.hash);
    }
}

impl fmt::Display for Syscall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.sp_name.is_empty() {
            write!(f, "@{}", self.sp_name)?;
        }
        write!(f, "(")?;
        for (i, p) in self.params.iter().enumerate() {
            write!(f, "{}", p)?;
            if i != self.params.len() - 1 {
                write!(f, ",")?;
            }
        }
        write!(f, ")")?;
        if let Some(ret) = self.ret.as_ref() {
            // TODO Store type name in TypeRef?
            write!(f, " -> {}", &self.types[ret])
        } else {
            Ok(())
        }
    }
}

pub struct SyscallBuilder {
    nr: Option<u32>,
    name: Option<Box<str>>,
    sp_name: Option<Box<str>>,
    params: Option<Box<[Param]>>,
    ret: Option<TypeId>,
    attrs: Option<SyscallAttr>,
}

impl SyscallBuilder {
    pub fn new() -> Self {
        todo!()
    }

    pub fn build(self) -> Syscall {
        todo!()
    }
}

pub struct Param {
    /// Name of param.
    pub name: Box<str>,
    /// Typeid of param.
    pub ty: TypeId,
    ///  Param is optional or not.
    pub optional: bool,
    // ...
    types: Rc<FxHashMap<TypeId, Type>>,
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ty = &self.types[&self.ty];
        write!(f, "{}:{}", self.name, ty)
    }
}
#[derive(Debug, Clone)]
pub struct SyscallAttr {
    timeout: Option<u32>,
    ignore_ret: bool,
    brk_ret: bool,
}

impl Default for SyscallAttr {
    fn default() -> Self {
        Self {
            timeout: None,
            ignore_ret: true,
            brk_ret: false,
        }
    }
}

pub type TypeId = usize;

pub struct Type;

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
