//! Data model of type, func, group and rule.

use std::fmt::{Display, Error, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Range;

use prettytable::Table;
use serde::{Deserialize, Serialize};

use crate::parse::Rule;

/// Type, group, rule def.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Items {
    pub types: Vec<Type>,
    pub groups: Vec<Group>,
    pub rules: Vec<RuleInfo>, // not used yet
}

impl Display for Items {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut stat_table = Table::new();
        stat_table.add_row(row!["Type", "Group", "Rule"]);
        stat_table.add_row(row!(self.types.len(), self.groups.len(), self.rules.len()));
        let stat_info = format!(
            "\n\t=====================STAT=====================\n{}",
            stat_table
        );

        let mut type_table = Table::new();
        type_table.add_row(row!["id", "type"]);
        for t in self.types.iter() {
            type_table.add_row(row![t.tid, t.info]);
        }
        let type_info = format!(
            "\n\t=====================TYPE=====================\n{}",
            type_table
        );

        let mut group_table = Table::new();
        group_table.add_row(row!["id", "name"]);
        for g in self.groups.iter() {
            group_table.add_row(row![g.id, g.ident]);
        }
        let group_info = format!(
            "\n\t=====================GROUP=====================\n{}",
            group_table
        );

        let mut fn_table = Table::new();
        fn_table.add_row(row!["id", "group", "prototype"]);
        for g in self.groups.iter() {
            for f in g.fns.iter() {
                fn_table.add_row(row![f.id, g.ident, f]);
            }
        }
        let fn_info = format!(
            "\n\t=====================FN=====================\n{}",
            fn_table
        );

        write!(f, "{}{}{}{}", stat_info, type_info, group_info, fn_info)
    }
}

impl Items {
    pub fn dump(&self) -> bincode::Result<Vec<u8>> {
        bincode::serialize(self)
    }

    pub fn load(b: &[u8]) -> bincode::Result<Self> {
        bincode::deserialize(b)
    }
}

/// Not sure if rule def is useful for program generation, so it's
/// not implemented yet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct RuleInfo;

impl Display for RuleInfo {
    fn fmt(&self, _f: &mut Formatter<'_>) -> Result<(), Error> {
        todo!()
    }
}

/// Id of type
pub type TypeId = u64;

/// Infomation of a type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Type {
    pub tid: TypeId,
    pub info: TypeInfo,
    // pub attrs: Option<Vec<Attr>>,
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}:{}", self.tid, self.info)
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.tid.eq(&other.tid)
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tid.hash(state)
    }
}

/// Information of a type expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
        str_type: StrType,
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

#[derive(Debug, Clone, PartialOrd, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StrType {
    Str,
    CStr,
    FileName,
}

impl Display for StrType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            StrType::Str => write!(f, "str"),
            StrType::CStr => write!(f, "cstr"),
            StrType::FileName => write!(f, "filename"),
        }
    }
}

impl Display for TypeInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            TypeInfo::Num(lim) => write!(f, "{:?}", lim),
            TypeInfo::Ptr { dir, tid, depth } => write!(
                f,
                "{}{} id({})",
                std::iter::repeat("*").take(*depth).collect::<String>(),
                *dir,
                *tid
            ),
            TypeInfo::Slice { tid, l, h } => match (*l, *h) {
                (-1, -1) => write!(f, "[id({})]", tid),
                (l, -1) => write!(f, "[id({});{}]", tid, l),
                (l, h) => write!(f, "[id({});({}:{})]", tid, l, h),
            },
            TypeInfo::Str { vals, str_type } => {
                let s: String = str_type.to_string();
                if let Some(ref vals) = vals {
                    write!(f, "{}{{{}}}", s, vals.join(","))
                } else {
                    write!(f, "{}", s)
                }
            }
            TypeInfo::Struct { ident, fields } => {
                let fields_str = fields
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "struct {}{{{}}}", ident, fields_str)
            }
            TypeInfo::Union { ident, fields } => {
                let fields_str = fields
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "union {}{{{}}}", ident, fields_str)
            }
            TypeInfo::Flag { ident, flags } => {
                let flags_str = flags
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "flag {}{{{}}}", ident, flags_str)
            }
            TypeInfo::Alias { ident, tid } => write!(f, "Alias {}=>id({})", ident, tid),
            TypeInfo::Res { tid } => write!(f, "res<id({})>", tid),
            TypeInfo::Len { tid, path, .. } => write!(f, "len<id({}),{}>", tid, path),
        }
    }
}

impl TypeInfo {
    pub fn primitive_types() -> Vec<TypeInfo> {
        vec![
            TypeInfo::Num(NumInfo::I8(NumLimit::None)),
            TypeInfo::Num(NumInfo::I16(NumLimit::None)),
            TypeInfo::Num(NumInfo::I32(NumLimit::None)),
            TypeInfo::Num(NumInfo::I64(NumLimit::None)),
            TypeInfo::Num(NumInfo::U8(NumLimit::None)),
            TypeInfo::Num(NumInfo::U16(NumLimit::None)),
            TypeInfo::Num(NumInfo::U32(NumLimit::None)),
            TypeInfo::Num(NumInfo::U64(NumLimit::None)),
            TypeInfo::Num(NumInfo::Usize(NumLimit::None)),
            TypeInfo::Num(NumInfo::Isize(NumLimit::None)),
            TypeInfo::Str {
                str_type: StrType::CStr,
                vals: None,
            },
            TypeInfo::Str {
                str_type: StrType::Str,
                vals: None,
            },
            TypeInfo::Str {
                str_type: StrType::FileName,
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

    pub fn str_info(str_type: StrType, vals: Option<Vec<String>>) -> Self {
        TypeInfo::Str { str_type, vals }
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

/// Id type of function declaration
pub type FnId = usize;

/// Information of function declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    // optional attrs for fn
    pub attrs: Option<Vec<Attr>>,
}

impl PartialEq for FnInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl Eq for FnInfo {}

impl Hash for FnInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl Display for FnInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut params_info = String::new();
        if let Some(ref params) = self.params {
            for p in params {
                params_info += &format!("{},", p);
            }
        }
        params_info.pop();
        let mut ret_info = String::new();
        if let Some(id) = self.r_tid {
            ret_info += &format!(" -> {}", id);
        }
        let attrs_str = match &self.attrs {
            None => "".into(),
            Some(attrs) => {
                let attrs = attrs
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("#[{}]", attrs)
            }
        };

        write!(
            f,
            "{}{}({}){}",
            attrs_str, self.dec_name, params_info, ret_info
        )
    }
}

impl FnInfo {
    pub fn new(
        id: FnId,
        gid: GroupId,
        name: &str,
        params: Option<Vec<Param>>,
        ret: Option<TypeId>,
        attrs: Option<Vec<Attr>>,
    ) -> Self {
        let dec_name: String = name.into();
        let call_name = dec_name.split('@').next().unwrap().into();
        FnInfo {
            id,
            gid,
            dec_name,
            call_name,
            params,
            r_tid: ret,
            attrs,
        }
    }

    pub fn gid(&mut self, gid: GroupId) -> &mut Self {
        self.gid = gid;
        self
    }

    pub fn attr(&mut self, attr: Attr) -> &mut Self {
        match self.attrs {
            None => self.attrs = Some(vec![attr]),
            Some(ref mut attrs) => attrs.push(attr),
        };
        self
    }

    pub fn attrs(&mut self, attrs: Option<Vec<Attr>>) -> &mut Self {
        self.attrs = attrs;
        self
    }

    pub fn has_params(&self) -> bool {
        self.params.is_some()
    }

    pub fn has_ret(&self) -> bool {
        self.r_tid.is_some()
    }

    pub fn iter_param(&self) -> impl Iterator<Item = &Param> + '_ {
        self.params.as_ref().unwrap().iter()
    }

    pub fn get_attr(&self, name: &str) -> Option<&Attr> {
        self.attrs
            .as_ref()
            .and_then(|attrs| attrs.iter().find(|&attr| attr.ident == name))
    }
}

/// Parameter of function
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Id of group
pub type GroupId = usize;

pub const DEFAULT_GID: GroupId = 0;
pub const DEFAULT_GROUP: &str = "@DEFAULT";

/// Information of group def
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub ident: String,
    pub attrs: Option<Vec<Attr>>,
    pub fns: Vec<FnInfo>,
}

impl Display for Group {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut table = Table::new();
        table.add_row(row!["id", "prototype"]);
        for f in self.fns.iter() {
            table.add_row(row![f.id, f]);
        }
        let attrs_str = match self.attrs {
            None => "".into(),
            Some(ref attrs) => {
                let attrs = attrs
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("#[{}]\n", attrs)
            }
        };
        write!(
            f,
            "{}Id:{} Name:{}\n\r{}",
            attrs_str, self.id, self.ident, table
        )
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
            id: DEFAULT_GID,
            ident: DEFAULT_GROUP.into(),
            fns: Vec::new(),
            attrs: None,
        }
    }
}

impl Group {
    pub fn new(id: GroupId, ident: &str) -> Self {
        Group {
            fns: vec![],
            ident: ident.into(),
            id,
            attrs: None,
        }
    }

    pub fn attrs(&mut self, attrs: Option<Vec<Attr>>) -> &mut Self {
        self.attrs = attrs;
        self
    }

    pub fn fn_info(&mut self, f_info: FnInfo) -> &mut Self {
        self.fns.push(f_info);
        self
    }

    pub fn attr(&mut self, attr: Attr) -> &mut Self {
        match self.attrs {
            None => self.attrs = Some(vec![attr]),
            Some(ref mut attrs) => attrs.push(attr),
        }
        self
    }

    pub fn add_fns(&mut self, f_info: impl IntoIterator<Item = FnInfo>) {
        self.fns.extend(f_info)
    }

    pub fn fn_num(&self) -> usize {
        self.fns.len()
    }

    pub fn iter_fn(&self) -> impl Iterator<Item = &FnInfo> + '_ {
        self.fns.iter()
    }

    pub fn index_by_name(&self, name: &str) -> Option<usize> {
        self.iter_fn().position(|f| f.dec_name == name)
    }

    pub fn index_by_id(&self, fid: FnId) -> Option<usize> {
        self.iter_fn().position(|f| f.id == fid)
    }
}

/// Information of different size numbe type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl Display for NumInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            NumInfo::I8(l) => write!(f, "i8{}", l),
            NumInfo::I16(l) => write!(f, "i16{}", l),
            NumInfo::I32(l) => write!(f, "i32{}", l),
            NumInfo::I64(l) => write!(f, "i64{}", l),
            NumInfo::U8(l) => write!(f, "u8{}", l),
            NumInfo::U16(l) => write!(f, "u16{}", l),
            NumInfo::U32(l) => write!(f, "u32{}", l),
            NumInfo::U64(l) => write!(f, "u64{}", l),
            NumInfo::Usize(l) => write!(f, "usize{}", l),
            NumInfo::Isize(l) => write!(f, "isize{}", l),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NumLimit<T> {
    Vals(Vec<T>),
    Range(Range<T>),
    None,
}

impl<T: Display> Display for NumLimit<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            NumLimit::Vals(ref vals) => {
                let vals_str = vals
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "{{{}}}", vals_str)
            }
            NumLimit::Range(ref rag) => write!(f, "({}:{})", rag.start, rag.end),
            NumLimit::None => write!(f, ""),
        }
    }
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
}

/// Direction of pointer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PtrDir {
    In,
    Out,
    InOut,
}

impl Display for PtrDir {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:?}", self)
    }
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

/// Field of struct or union
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Field {
    pub ident: String,
    pub tid: TypeId,
}

impl Display for Field {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}:id({})", self.ident, self.tid)
    }
}

impl Field {
    pub fn new(ident: &str, tid: TypeId) -> Self {
        Field {
            ident: String::from(ident),
            tid,
        }
    }
}

/// Signle flag in flag deleration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Flag {
    pub ident: String,
    pub val: i64,
}

impl Display for Flag {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}={}", self.ident, self.val)
    }
}

impl Flag {
    pub fn new(ident: &str, val: i64) -> Self {
        Self {
            ident: ident.into(),
            val,
        }
    }
}

/// Attribute of group or function
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Attr {
    pub ident: String,
    pub vals: Option<Vec<String>>,
}

impl Display for Attr {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let vals_str = match self.vals {
            Some(ref vals) => vals.join(","),
            None => "".into(),
        };
        write!(f, "{}({})", self.ident, vals_str)
    }
}

impl Attr {
    pub fn new(ident: &str) -> Self {
        Attr {
            ident: ident.into(),
            vals: None,
        }
    }

    pub fn has_vals(&self) -> bool {
        self.vals.is_some() && !self.vals.as_ref().unwrap().is_empty()
    }

    pub fn iter_val(&self) -> impl Iterator<Item = &str> + '_ {
        assert!(self.has_vals());
        self.vals.as_ref().unwrap().iter().map(|v| v as &str)
    }
}
