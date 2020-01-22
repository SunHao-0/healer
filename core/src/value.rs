use rand::prelude::SliceRandom;
use rand::thread_rng;

use fots::types::{TypeId, TypeInfo};

use crate::gen::gen_slice_len;
use crate::prog::CId;
use crate::target::Target;
use crate::value::NumValue::Unsigned;

/// Value of type
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum NumValue {
    Signed(i64),
    Unsigned(u64),
}

impl Value {
    pub fn default_val(tid: TypeId, t: &Target) -> Value {
        match t.type_of(tid) {
            TypeInfo::Num(..) => Value::Num(Unsigned(0)),
            TypeInfo::Ptr { .. } => Value::Ptr(None),
            TypeInfo::Slice { tid, l, h } => {
                let len: usize = gen_slice_len(*l, *h);
                let mut vals = Vec::new();
                for _ in 0..len {
                    vals.push(Value::default_val(*tid, t));
                }
                Value::Group(vals)
            }
            TypeInfo::Str { .. } => Value::Str(String::new()),
            TypeInfo::Struct { fields, .. } => {
                let mut vals = Vec::new();
                for field in fields.iter() {
                    vals.push(Value::default_val(field.tid, t));
                }
                Value::Group(vals)
            }
            TypeInfo::Union { fields, .. } => {
                let field = fields.choose(&mut thread_rng()).unwrap();
                Value::Opt(Box::new(Value::default_val(field.tid, t)))
            }
            TypeInfo::Flag { flags, .. } => {
                let flag_val = flags.choose(&mut thread_rng()).unwrap();
                Value::Num(NumValue::Signed(flag_val.val))
            }
            TypeInfo::Alias { tid, .. } => Value::default_val(*tid, t),
            TypeInfo::Res { tid } => Value::default_val(*tid, t),
            TypeInfo::Len { .. } => Value::Num(NumValue::Unsigned(0)),
        }
    }
}
