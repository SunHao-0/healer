use std::ops::Index;

use fots::types::{FnId, GroupId, TypeId};

use crate::value::Value;

pub type CId = u64;
/// Index for indexing arg in a prog
pub type ArgIndex = (usize, usize);

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

impl Index<ArgIndex> for Prog {
    type Output = Arg;

    fn index(&self, index: ArgIndex) -> &Self::Output {
        let c = &self.calls[index.0];
        if index.1 == c.args.len() {
            c.ret.as_ref().unwrap()
        } else {
            &c.args[index.1]
        }
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
