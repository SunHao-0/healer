use crate::prog::CId;

/// Value of type
pub enum Value {
    /// Value for number,flag,len
    Num(NumValue),
    /// Value for str,cstr,filename
    Str(String),
    /// Value for slice, struct
    Group(Vec<Value>),
    /// Value for union
    Opt(Box<Value>),
    /// Value for ptr
    Ptr(Option<Box<Value>>),
    /// Value for res type
    Res { ref_id: CId, refed_ids: Vec<CId> },
}

pub enum NumValue {
    Signed(i64),
    Unsigned(u64),
}
