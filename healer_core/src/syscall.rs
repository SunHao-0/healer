//! AST of Syzlang description

use crate::ty::{Field, Type};
use std::fmt::Display;

pub type SyscallId = usize;
pub const MAX_PARAMS_NUM: usize = 9;

#[derive(Debug, Clone)]
pub struct Syscall {
    /// Unique id of each declared syscall.
    id: SyscallId,
    /// Kernel call number.
    nr: u64,
    /// Name in syslang description.
    name: Box<str>,
    /// Syscall name.
    call_name: Box<str>,
    /// Syz: number of trailing args that should be zero-filled
    missing_args: u64,
    params: Box<[Field]>,
    ret: Option<Type>,
    attr: SyscallAttr,
}

impl Display for Syscall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}(", self.name)?;
        for (i, param) in self.params.iter().enumerate() {
            write!(f, "{}", param)?;
            if i != self.params.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")?;
        if let Some(r) = self.ret.as_ref() {
            write!(f, " {}", r.name())?;
        }
        if !self.attr.is_default() {
            write!(f, " ({})", self.attr)?;
        }
        Ok(())
    }
}

impl Syscall {
    #[inline(always)]
    pub fn id(&self) -> SyscallId {
        self.id
    }

    #[inline(always)]
    pub fn nr(&self) -> u64 {
        self.nr
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline(always)]
    pub fn call_name(&self) -> &str {
        &self.call_name
    }

    #[inline(always)]
    pub fn missing_args(&self) -> u64 {
        self.missing_args
    }

    #[inline(always)]
    pub fn params(&self) -> &[Field] {
        &self.params
    }

    #[inline(always)]
    pub fn ret(&self) -> Option<&Type> {
        self.ret.as_ref()
    }

    #[inline(always)]
    pub fn attr(&self) -> &SyscallAttr {
        &self.attr
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyscallBuilder {
    id: Option<SyscallId>,
    nr: Option<u64>,
    name: Option<String>,
    call_name: Option<String>,
    missing_args: Option<u64>,
    params: Vec<Field>,
    ret: Option<Type>,
    attr: Option<SyscallAttr>,
}

impl SyscallBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(&mut self, id: SyscallId) -> &mut Self {
        self.id = Some(id);
        self
    }

    pub fn nr(&mut self, nr: u64) -> &mut Self {
        self.nr = Some(nr);
        self
    }

    pub fn name<T: Into<String>>(&mut self, name: T) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn call_name<T: Into<String>>(&mut self, call_name: T) -> &mut Self {
        self.call_name = Some(call_name.into());
        self
    }

    pub fn missing_args(&mut self, missing_args: u64) -> &mut Self {
        self.missing_args = Some(missing_args);
        self
    }

    pub fn params(&mut self, params: Vec<Field>) -> &mut Self {
        self.params = params;
        self
    }

    pub fn ret(&mut self, ret: Type) -> &mut Self {
        self.ret = Some(ret);
        self
    }

    pub fn attr(&mut self, attr: SyscallAttr) -> &mut Self {
        self.attr = Some(attr);
        self
    }

    pub fn build(self) -> Syscall {
        Syscall {
            id: self.id.unwrap(),
            nr: self.nr.unwrap_or_default(),
            name: self.name.clone().unwrap().into_boxed_str(),
            call_name: if let Some(call_name) = self.call_name {
                call_name.into_boxed_str()
            } else {
                self.name.unwrap().into_boxed_str()
            },
            missing_args: self.missing_args.unwrap_or_default(),
            params: self.params.into_boxed_slice(),
            ret: self.ret,
            attr: self.attr.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SyscallAttr {
    pub disabled: bool,
    pub timeout: u64,
    pub prog_timeout: u64,
    pub ignore_return: bool,
    pub breaks_returns: bool,
}

impl Default for SyscallAttr {
    fn default() -> Self {
        Self {
            disabled: false,
            timeout: 0,
            prog_timeout: 0,
            ignore_return: false,
            breaks_returns: false,
        }
    }
}

impl SyscallAttr {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Display for SyscallAttr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        let mut out = String::new();
        if self.disabled {
            write!(out, "disabled, ")?;
        }
        if self.timeout != 0 {
            write!(out, "timeout[{}], ", self.timeout)?;
        }
        if self.prog_timeout != 0 {
            write!(out, "prog_timeout[{}], ", self.prog_timeout)?;
        }
        if self.ignore_return {
            write!(out, "ignore_return, ")?;
        }
        if self.breaks_returns {
            write!(out, "breaks_returns, ")?;
        }
        write!(f, "{}", out.trim().trim_matches(','))
    }
}
