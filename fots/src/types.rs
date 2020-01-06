use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Range;

use prettytable::Table;

use crate::grammar::Rule;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Items {
    pub types: Vec<Type>,
    pub groups: Vec<Group>,
    pub rules: Vec<RuleInfo>,
}

impl Display for Items {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "\n=====================Type=====================\n");
        let mut type_table = Table::new();
        type_table.add_row(row!["id","type"]);
        for t in self.types.iter() {
            type_table.add_row(row![t.tid,t.info]);
        }
        write!(f, "{}", type_table);
        write!(f, "\n=====================GROUP=====================\n");
        for g in self.groups.iter() {
            write!(f, "{}", g);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleInfo {}

impl Display for RuleInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "rule:{:?}", self)
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
    Num(NumInfo),
    // Ptr type. utp stands for  under type
    Ptr {
        dir: PtrDir,
        depth: usize,
        tid: TypeId,
    },
    // Slice type. If range specified,use (l,h) as range. if len specified, l is len,h is -1.
    Slice {
        tid: TypeId,
        l: isize,
        h: isize,
    },
    Str {
        c_style: bool,
        vals: Option<Vec<String>>,
    },
    Struct {
        ident: String,
        fields: Vec<Field>,
    },
    Union {
        ident: String,
        fields: Vec<Field>,
    },
    Flag {
        ident: String,
        flags: Vec<Flag>,
    },
    Alias {
        ident: String,
        tid: TypeId,
    },
    Res {
        tid: TypeId,
    },
    Len {
        tid: TypeId,
        path: String,
        is_param: bool,
    },
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:?}", self)
    }
}

impl TypeInfo {
    pub fn primitive_types() -> Vec<TypeInfo> {
        vec![
            TypeInfo::Num(NumInfo::I8(NumLimit::None)),
            TypeInfo::Num(NumInfo::I8(NumLimit::None)),
            TypeInfo::Num(NumInfo::I32(NumLimit::None)),
            TypeInfo::Num(NumInfo::I64(NumLimit::None)),
            TypeInfo::Num(NumInfo::U8(NumLimit::None)),
            TypeInfo::Num(NumInfo::U16(NumLimit::None)),
            TypeInfo::Num(NumInfo::U32(NumLimit::None)),
            TypeInfo::Num(NumInfo::U64(NumLimit::None)),
            TypeInfo::Num(NumInfo::Usize(NumLimit::None)),
            TypeInfo::Num(NumInfo::Isize(NumLimit::None)),
            TypeInfo::Str {
                c_style: false,
                vals: None,
            },
            TypeInfo::Str {
                c_style: true,
                vals: None,
            },
        ]
    }

    pub fn default_ptr(tid: TypeId) -> TypeInfo {
        TypeInfo::Ptr {
            dir: PtrDir::In,
            depth: 1,
            tid,
        }
    }

    pub fn default_slice(tid: TypeId) -> TypeInfo {
        TypeInfo::Slice { l: -1, h: -1, tid }
    }

    pub fn ident(&self) -> Option<&str> {
        match self {
            Self::Struct { ident, .. } => Some(ident),
            Self::Union { ident, .. } => Some(ident),
            Self::Alias { ident, .. } => Some(ident),
            Self::Flag { ident, .. } => Some(ident),
            _ => None,
        }
    }

    pub fn slice_info(tid: TypeId, (l, h): (isize, isize)) -> TypeInfo {
        TypeInfo::Slice { tid, l, h }
    }

    pub fn str_info(c_style: bool, vals: Option<Vec<String>>) -> Self {
        TypeInfo::Str { c_style, vals }
    }
    pub fn ptr_info(tid: TypeId, dir: PtrDir, depth: usize) -> Self {
        TypeInfo::Ptr { tid, dir, depth }
    }

    pub fn len_info(tid: TypeId, path: &str) -> Self {
        // TODO path parse
        TypeInfo::Len {
            path: String::from(path),
            is_param: true,
            tid,
        }
    }

    pub fn res_info(tid: TypeId) -> TypeInfo {
        TypeInfo::Res { tid }
    }

    pub fn struct_info(ident: &str, fields: Vec<Field>) -> Self {
        TypeInfo::Struct {
            ident: String::from(ident),
            fields,
        }
    }

    pub fn union_info(ident: &str, fields: Vec<Field>) -> Self {
        TypeInfo::Union {
            ident: String::from(ident),
            fields,
        }
    }

    pub fn alias_info(ident: &str, tid: TypeId) -> Self {
        TypeInfo::Alias {
            ident: String::from(ident),
            tid,
        }
    }

    pub fn flag_info(ident: &str, flags: Vec<Flag>) -> Self {
        TypeInfo::Flag {
            ident: ident.into(),
            flags,
        }
    }
}

pub type FnId = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnInfo {
    pub id: FnId,
    pub gid: GroupId,
    // Name declared in source file
    pub dec_name: String,
    // actual called name
    pub call_name: String,
    // input params
    pub params: Option<Vec<Param>>,
    // id of return type
    pub r_tid: Option<TypeId>,
}

impl Display for FnInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} (", self.dec_name);
        if let Some(ref params) = self.params {
            for p in params {
                write!(f, "{},", p);
            }
        }
        write!(f, ")");
        if let Some(id) = self.r_tid {
            write!(f, "{}", id);
        }
        Ok(())
    }
}

impl FnInfo {
    pub fn new(
        id: FnId,
        gid: GroupId,
        name: &str,
        params: Option<Vec<Param>>,
        ret: Option<TypeId>,
    ) -> Self {
        let dec_name: String = name.into();
        let call_name = dec_name.split('$').next().unwrap().into();
        FnInfo {
            id,
            gid,
            dec_name,
            call_name,
            params,
            r_tid: ret,
        }
    }

    pub fn gid(&mut self, gid: GroupId) -> &mut Self {
        self.gid = gid;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Param {
    pub ident: String,
    pub tid: TypeId,
}

impl Display for Param {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}:{}", self.ident, self.tid)
    }
}

impl Param {
    pub fn new(ident: &str, tid: TypeId) -> Self {
        Param {
            ident: ident.into(),
            tid,
        }
    }
}

pub type GroupId = usize;

#[derive(Debug, Clone)]
pub struct Group {
    pub id: GroupId,
    pub ident: String,
    pub fns: Vec<FnInfo>,
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut table = Table::new();
        table.add_row(row!["Group id","Group name"]);
        table.add_row(row![self.id,self.ident]);
        table.add_row(row!["Fn id ","fn prototype"]);
        for f in self.fns.iter() {
            table.add_row(row![f.id,f]);
        }
        write!(f, "{}", table)
    }
}

impl PartialEq for Group {
    fn eq(&self, other: &Self) -> bool {
        self.ident.eq(&other.ident)
    }
}

impl Eq for Group {}

impl Hash for Group {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.ident.hash(state);
    }
}

impl Default for Group {
    fn default() -> Self {
        Self {
            id: 0,
            ident: "Default".into(),
            fns: Vec::new(),
        }
    }
}

impl Group {
    pub fn new(id: GroupId, ident: &str) -> Self {
        Group {
            fns: vec![],
            ident: ident.into(),
            id,
        }
    }

    pub fn add(&mut self, f_info: FnInfo) {
        self.fns.push(f_info)
    }

    pub fn add_fns(&mut self, f_info: impl IntoIterator<Item=FnInfo>) {
        self.fns.extend(f_info)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NumInfo {
    I8(NumLimit<i8>),
    I16(NumLimit<i16>),
    I32(NumLimit<i32>),
    I64(NumLimit<i64>),
    U8(NumLimit<u8>),
    U16(NumLimit<u16>),
    U32(NumLimit<u32>),
    U64(NumLimit<u64>),
    Usize(NumLimit<usize>),
    Isize(NumLimit<isize>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NumLimit<T> {
    Vals(Vec<T>),
    Range(Range<T>),
    None,
}

impl NumInfo {
    pub fn from_rule(ru: Rule) -> Self {
        match ru {
            Rule::I8 => Self::I8(NumLimit::None),
            Rule::I16 => Self::I16(NumLimit::None),
            Rule::I32 => Self::I32(NumLimit::None),
            Rule::I64 => Self::I64(NumLimit::None),
            Rule::U8 => Self::U8(NumLimit::None),
            Rule::U16 => Self::U16(NumLimit::None),
            Rule::U32 => Self::U32(NumLimit::None),
            Rule::U64 => Self::U64(NumLimit::None),
            Rule::Usize => Self::Usize(NumLimit::None),
            Rule::Isize => Self::Isize(NumLimit::None),
            _ => unreachable!(),
        }
    }

    pub fn change_limit_i8(&mut self, limit: NumLimit<i8>) {
        match self {
            Self::I8(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_i16(&mut self, limit: NumLimit<i16>) {
        match self {
            Self::I16(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_i32(&mut self, limit: NumLimit<i32>) {
        match self {
            Self::I32(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_i64(&mut self, limit: NumLimit<i64>) {
        match self {
            Self::I64(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_u8(&mut self, limit: NumLimit<u8>) {
        match self {
            Self::U8(l) => *l = limit,
            _ => unreachable!(),
        }
    }

    pub fn change_limit_u16(&mut self, limit: NumLimit<u16>) {
        match self {
            Self::U16(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_u32(&mut self, limit: NumLimit<u32>) {
        match self {
            Self::U32(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_u64(&mut self, limit: NumLimit<u64>) {
        match self {
            Self::U64(l) => *l = limit,
            _ => unreachable!(),
        }
    }

    pub fn change_limit_usize(&mut self, limit: NumLimit<usize>) {
        match self {
            Self::Usize(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn change_limit_isize(&mut self, limit: NumLimit<isize>) {
        match self {
            Self::Isize(l) => *l = limit,
            _ => unreachable!(),
        }
    }
    pub fn limit_mut<T>(&mut self) -> &mut NumLimit<T> {
        todo!()
    }

    //    pub fn size(&self) -> usize{
    //        match self{
    //            I8=> 8,
    //            I16(NumLimit<i16>),
    //            I32(NumLimit<i32>),
    //            I64(NumLimit<i64>),
    //            U8(NumLimit<u8>),
    //            U16(NumLimit<u16>),
    //            U32(NumLimit<u32>),
    //            U64(NumLimit<u64>),
    //            Usize(NumLimit<usize>),
    //            Isize(NumLimit<isize>),
    //        }
    //    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PtrDir {
    In,
    Out,
    InOut,
}

impl PtrDir {
    pub fn from_rule(dir: Rule) -> Self {
        match dir {
            Rule::In => Self::In,
            Rule::Out => Self::Out,
            Rule::InOut => Self::InOut,
            _ => unreachable!(),
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Flag {
    pub ident: String,
    pub val: i32,
}

impl Flag {
    pub fn new(ident: &str, val: i32) -> Self {
        Self {
            ident: ident.into(),
            val,
        }
    }
}
