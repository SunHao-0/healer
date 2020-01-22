use fots::types::{FnId, GroupId, TypeId};

use crate::value::Value;

pub type CId = u64;

#[derive(Debug, Clone)]
pub struct Prog {
    pub gid: GroupId,
    pub calls: Vec<Call>,
}

impl Prog {
    pub fn new(gid: GroupId) -> Self {
        Self {
            gid,
            calls: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }
}

#[derive(Debug, Clone)]
pub struct Call {
    /// prototype
    pub fid: FnId,
    pub args: Vec<Arg>,
    pub ret: Option<Arg>,
}

impl Call {
    pub fn new(fid: FnId) -> Self {
        Self {
            args: Vec::new(),
            ret: None,
            fid,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub tid: TypeId,
    pub val: Value,
}
