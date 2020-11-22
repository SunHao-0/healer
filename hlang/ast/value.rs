//! Abstract representation or AST of system call model.
use super::types::Type;
use super::Dir;
use std::fmt;
use std::rc::Rc;

pub struct Value {
    pub dir: Dir,
    pub ty: Rc<Type>,
    pub kind: ValueKind,
}

pub enum ValueKind {
    Scalar(u64),
    Ptr { addr: u64, pointee: Box<Value> },
    Vma { addr: u64, size: u64 },
    Bytes(Box<[u8]>),
    Group(Vec<Value>),
    Ref,
}

impl fmt::Display for Value {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        todo!()
    }
}
