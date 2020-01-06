use std::ops::Range;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct Fots {
    types: TTable,
    groups: Vec<Group>,
    rules: Vec<RuleInfo>,
}

impl Fots {
    pub fn new() -> Self {
        todo!()
    }
}

pub struct RuleInfo {}

pub struct TTable {
    pub ids: HashMap<TypeId, Rc<Type>>,
    pub infos: HashSet<Rc<Type>>,
    pub symbols: HashMap<TypeId, String>,

}

impl TTable {
    pub fn new() -> Self {
        TTable {
            ids: HashMap::new(),
            symbols: HashMap::new(),
            infos: HashSet::new(),
        }
    }

    pub fn add(t: Type) {
        todo!()
    }
}

pub type TypeId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Type {
    pub tid: TypeId,
    pub info: TypeInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeInfo {
    Num { base: NumType, vals: Option<Vec<i64>>, range: Option<Range<i64>> },
    // Ptr type. utp stands for  under type
    Ptr { dir: PtrDir, tid: TypeId },
    // Slice type. If range specified,use (l,h) as range. if len specified, l is len,h is -1.
    Slice { tid: TypeId, l: isize, h: isize },
    Str { c_style: bool, vals: Option<Vec<String>> },
    Struct { ident: String, fields: Vec<Field> },
    Union { ident: String, fields: Vec<Field> },
    Flag { ident: String, vals: Vec<i32>, idents: Vec<String> },
    Alias { ident: String, tp: TypeId },
}

impl TypeInfo {
    pub fn struct_info(ident: &str) -> Self {
        TypeInfo::Struct { ident: String::from(ident), fields: Vec::new() }
    }
    pub fn add_field(&mut self, field: Field) {
        use TypeInfo::*;
        match self {
            Struct { ref mut fields, .. } => { fields.push(field) }
            Union { ref mut fields, .. } => { fields.push(field) }
            _ => unreachable!()
        }
    }
}

pub struct FnInfo {
    pub id: usize,
    // Name declared in source file
    pub dec_name: String,
    // actual called name
    pub call_name: String,
    // input params
    pub params: Option<Vec<Param>>,
    // id of return type
    pub r_tid: Option<TypeId>,
}

pub struct Param {
    pub ident: String,
    pub tid: TypeId,
}

pub struct Group {
    pub ident: String,
    pub fns: Vec<FnInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    Usize,
    Isize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PtrDir {
    In,
    Out,
    Inout,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub ident: String,
    pub tid: TypeId,
}

impl Field {
    pub fn new(ident: &str, tid: TypeId) -> Self {
        Field {
            ident: String::from(ident),
            tid,
        }
    }
}

