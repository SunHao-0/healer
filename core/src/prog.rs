use fots::types::{FnId, TypeId};

use crate::value::Value;

pub type CId = u64;

pub struct Prog {
    pub calls: Vec<Call>,
}

impl Prog {
    pub fn new() -> Self {
        Self {
            calls: Vec::new()
        }
    }

    pub fn len(&self) -> usize {
        self.len()
    }
}

pub struct Call {
    /// prototype
    pub fid: FnId,
    pub args: Vec<Arg>,
    pub ret: Option<Arg>,
}

pub struct Arg {
    tid: TypeId,
    val: Value,
}
