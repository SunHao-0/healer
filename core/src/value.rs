use rand::prelude::SliceRandom;
use rand::{thread_rng, Rng};

use fots::types::{TypeId, TypeInfo};

use crate::gen::gen_slice_len;
use crate::prog::ArgIndex;
use crate::target::Target;

/// Value of type
#[derive(Debug, Clone)]
pub enum Value {
    /// Value that stores num, both signed and unsigned but not bigger then 8 bytes
    Num(NumValue),
    /// Value that stores utf-8 encoded value
    Str(String),
    /// Combined value
    Group(Vec<Value>),
    /// Value for union
    Opt { choice: usize, val: Box<Value> },
    /// Value that ref other argument
    Ref(ArgIndex),
    /// Nothing
    None,
}

#[derive(Debug, Clone)]
pub enum NumValue {
    Signed(i64),
    Unsigned(u64),
}

impl Value {
    pub fn default_val(tid: TypeId, t: &Target) -> Value {
        use NumValue::*;

        let mut rng = thread_rng();
        match t.type_of(tid) {
            TypeInfo::Num(..) => Value::Num(Unsigned(0)),
            TypeInfo::Ptr { .. } => Value::None,
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
                let field_i = rng.gen_range(0, fields.len());
                let field = &fields[field_i];
                Value::Opt {
                    choice: field_i,
                    val: Box::new(Value::default_val(field.tid, t)),
                }
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
