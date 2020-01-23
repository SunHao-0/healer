use std::ops::Index;

use fots::types::{FnId, GroupId, TypeId};

use crate::value::Value;

/// Id of call in a prog
pub type CId = usize;
/// Index for indexing arg in a prog
pub type ArgIndex = (CId, ArgPos);

/// Position of arg in a call
#[derive(Clone, Debug)]
pub enum ArgPos {
    Arg(usize),
    Ret,
}

/// Seq of call of a group
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

    #[inline]
    pub fn add_call(&mut self, call: Call) -> &mut Call {
        self.calls.push(call);
        self.calls.last_mut().unwrap()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
}

impl Index<ArgIndex> for Prog {
    type Output = Arg;

    fn index(&self, index: ArgIndex) -> &Self::Output {
        let c = &self.calls[index.0];
        match index.1 {
            ArgPos::Arg(i) => &c.args[i],
            ArgPos::Ret => c.ret.as_ref().unwrap(),
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

    #[inline]
    pub fn add_arg(&mut self, arg: Arg) -> &mut Arg {
        self.args.push(arg);
        self.args.last_mut().unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub tid: TypeId,
    pub val: Value,
}

impl Arg {
    pub fn new(tid: TypeId) -> Self {
        Self {
            tid,
            val: Value::None,
        }
    }
}
