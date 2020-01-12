//! Parser for items

use std::collections::HashMap;
use std::error::Error;
use std::ops::Range;
use std::process::exit;

use num_traits::Num;
use pest::iterators::{Pair, Pairs};

use crate::{num, parse_grammar};
use crate::errors;
use crate::grammar::Rule;
use crate::types::{
    Attr, DEFAULT_GID, Field, Flag, FnId, FnInfo, Group, GroupId, Items, NumInfo, NumLimit, Param, PtrDir,
    Type, TypeId, TypeInfo,
};

/// Parse plain text based on grammar, return all declarations in text
pub fn parse(text: &str) -> Result<Items, errors::Error> {
    // parse grammar
    let parse_tree = parse_grammar(text)?;
    Parser::parse(parse_tree)
}

struct Parser {
    type_table: TypeTable,
    group_table: GroupTable,
    fid_count: FnId,
}

impl Parser {
    fn new() -> Self {
        Parser {
            type_table: TypeTable::with_primitives(),
            group_table: Default::default(),
            fid_count: 0,
        }
    }

    pub fn parse(decls: Pairs<Rule>) -> Result<Items, errors::Error> {
        let mut parser = Parser::new();
        for p in decls {
            match p.as_rule() {
                Rule::TypeDef => {
                    parser.parse_type(p);
                }
                Rule::FuncDef => parser.parse_default_group(p),
                Rule::GroupDef => parser.parse_group(p),
                Rule::RuleDef => parser.parse_rule(p),
                Rule::EOI => break,
                _ => unreachable!(),
            }
        }
        parser.finish()
    }

    fn finish(self) -> Result<Items, errors::Error> {
        let idents_err = self.type_table.check();
        if idents_err.is_some() {
            return Err(idents_err.unwrap());
        }
        let mut items = Items {
            types: self.type_table.into_types(),
            groups: self.group_table.into_groups(),
            rules: Vec::new(),
        };
        items.types.sort_by_key(|a| a.tid);
        items.groups.sort_by_key(|i| i.id);
        Ok(items)
    }

    fn parse_default_group(&mut self, p: Pair<Rule>) {
        let fn_info = self.parse_func(p, DEFAULT_GID);
        self.group_table.add_fn(DEFAULT_GID, fn_info);
    }

    fn parse_func(&mut self, p: Pair<Rule>, gid: GroupId) -> FnInfo {
        let mut p = p.into_inner();
        let mut attrs = None;
        let attr_or_ident_p = p.next().unwrap();
        let ident = match attr_or_ident_p.as_rule() {
            Rule::FuncIdent => attr_or_ident_p.as_str(),
            Rule::AttrsDef => {
                attrs = Some(self.parse_attrs(attr_or_ident_p));
                p.next().unwrap().as_str()
            }
            _ => unreachable!(),
        };
        let mut params = None;
        let mut ret = None;
        for p in p {
            match p.as_rule() {
                Rule::ParamsDec => params = Some(self.parse_params(p)),
                Rule::TypeExp => ret = Some(self.parse_type_exp(p)),
                _ => unreachable!(),
            }
        }
        FnInfo::new(self.next_fid(), gid, ident, params, ret, attrs)
    }

    fn parse_attrs(&mut self, p: Pair<Rule>) -> Vec<Attr> {
        p.into_inner().map(|p| self.parse_attr(p)).collect()
    }

    fn parse_attr(&mut self, p: Pair<Rule>) -> Attr {
        let mut p = p.into_inner();
        let attr_name = p.next().unwrap().as_str();
        let vals = if let Some(p) = p.next() {
            Some(
                p.into_inner()
                    .map(|p| p.as_str().to_string())
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        Attr {
            ident: attr_name.into(),
            vals,
        }
    }

    fn parse_params(&mut self, p: Pair<Rule>) -> Vec<Param> {
        p.into_inner().map(|p| self.parse_param(p)).collect()
    }

    fn parse_param(&mut self, p: Pair<Rule>) -> Param {
        let mut p = p.into_inner();
        Param::new(
            p.next().unwrap().as_str(),
            self.parse_type_exp(p.next().unwrap()),
        )
    }

    fn parse_group(&mut self, p: Pair<Rule>) {
        let mut p = p.into_inner();
        let mut attr = None;
        let attr_or_ident_p = p.next().unwrap();
        let ident = match attr_or_ident_p.as_rule() {
            Rule::Ident => attr_or_ident_p.as_str(),
            Rule::AttrsDef => {
                attr = Some(self.parse_attrs(attr_or_ident_p));
                p.next().unwrap().as_str()
            }
            _ => unreachable!(),
        };
        let gid = self.group_table.id_of(ident);
        self.group_table.get_mut(gid).unwrap().attrs(attr);
        let fns = p.map(|p| self.parse_func(p, gid)).collect::<Vec<_>>();
        self.group_table.add_fns(gid, fns);
    }

    fn parse_rule(&mut self, _p: Pair<Rule>) {
        todo!("Parsing of rule")
    }

    fn parse_type(&mut self, p: Pair<Rule>) -> TypeId {
        let p: Pair<Rule> = p.into_inner().next().unwrap();

        match p.as_rule() {
            Rule::StructDef => self.parse_struct(p),
            Rule::UnionDef => self.parse_union(p),
            Rule::FlagDef => self.parse_flag(p),
            Rule::AliasDef => self.parse_alias(p),
            _ => unreachable!(),
        }
    }

    fn parse_struct(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p: Pairs<Rule> = p.into_inner();
        let ident_p: Pair<Rule> = p.next().unwrap();
        let fields_p: Pair<Rule> = p.next().unwrap();
        assert_eq!(fields_p.as_rule(), Rule::Fields);
        let info = TypeInfo::struct_info(ident_p.as_str(), self.parse_fields(fields_p));
        self.type_table.add(info)
    }

    fn parse_fields(&mut self, p: Pair<Rule>) -> Vec<Field> {
        p.into_inner().map(|p| self.parse_field(p)).collect()
    }

    fn parse_field(&mut self, p: Pair<Rule>) -> Field {
        let mut field_p = p.into_inner();
        let ident_p = field_p.next().unwrap();
        let tid = self.parse_type_exp(field_p.next().unwrap());
        Field {
            ident: String::from(ident_p.as_str()),
            tid,
        }
    }

    fn parse_type_exp(&mut self, p: Pair<Rule>) -> TypeId {
        let p = p.into_inner().next().unwrap();
        // SliceCtr | PtrCtr | ResCtr | LenCtr | NamedType
        match p.as_rule() {
            Rule::SliceCtr => self.parse_slice(p),
            Rule::PtrCtr => self.parse_ptr(p),
            Rule::ResCtr => self.parse_res(p),
            Rule::LenCtr => self.parse_len(p),
            Rule::NamedType => self.parse_name_type(p),
            _ => unreachable!(),
        }
    }
    #[allow(unused_assignments)]
    fn parse_ptr(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let mut dir_or_exp = p.next().unwrap();
        let mut dir = PtrDir::In;

        match dir_or_exp.as_rule() {
            Rule::In | Rule::Out | Rule::InOut => {
                dir = PtrDir::from_rule(dir_or_exp.as_rule());
                dir_or_exp = p.next().unwrap() // to exp pair
            }
            _ => {}
        }

        let mut depth = 1;
        let mut tid: TypeId = 0;
        loop {
            dir_or_exp = dir_or_exp.into_inner().next().unwrap();
            match dir_or_exp.as_rule() {
                Rule::PtrCtr => {
                    depth += 1;
                    dir_or_exp = dir_or_exp.into_inner().next().unwrap();
                    match dir_or_exp.as_rule() {
                        Rule::In | Rule::Out | Rule::InOut => {
                            panic!("In/Out only can be use to top pointer")
                        }
                        _ => (),
                    }
                    continue;
                }
                Rule::SliceCtr => {
                    tid = self.parse_slice(dir_or_exp);
                    break;
                }
                Rule::ResCtr => {
                    tid = self.parse_res(dir_or_exp);
                    break;
                }
                Rule::LenCtr => {
                    tid = self.parse_len(dir_or_exp);
                    break;
                }
                Rule::NamedType => {
                    tid = self.parse_name_type(dir_or_exp);
                    break;
                }
                _ => unreachable!(),
            }
        }
        self.type_table.add(TypeInfo::ptr_info(tid, dir, depth))
    }

    fn parse_res(&mut self, p: Pair<Rule>) -> TypeId {
        let p = p.into_inner().next().unwrap();
        assert_eq!(p.as_rule(), Rule::TypeExp);
        let tid = self.parse_type_exp(p);
        self.type_table.add(TypeInfo::res_info(tid))
    }

    fn parse_len(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let tid = self.parse_num_type(p.next().unwrap());
        let path = p.next().unwrap().as_str();
        self.type_table.add(TypeInfo::len_info(tid, path))
    }

    fn parse_name_type(&mut self, p: Pair<Rule>) -> TypeId {
        let p = p.into_inner().next().unwrap();
        match p.as_rule() {
            //NumType | StrType | Ident
            Rule::NumType => self.parse_num_type(p),
            Rule::StrType => self.parse_str_type(p),
            Rule::Ident => self.parse_ident(p),
            _ => unreachable!(),
        }
    }

    fn parse_ident(&mut self, p: Pair<Rule>) -> TypeId {
        self.type_table.id_of(p.as_str())
    }

    fn parse_num_type(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let t_p = p.next().unwrap();
        let mut num_info = NumInfo::from_rule(t_p.as_rule());
        match p.next() {
            Some(l_p) => match l_p.as_rule() {
                Rule::Range => match &mut num_info {
                    NumInfo::I8(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::I16(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::I32(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::I64(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::U8(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::U16(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::U32(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::U64(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::Usize(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                    NumInfo::Isize(l) => {
                        *l = NumLimit::Range(self.parse_range(l_p));
                    }
                },
                Rule::NumVals => match &mut num_info {
                    NumInfo::I8(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::I16(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::I32(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::I64(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::U8(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::U16(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::U32(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::U64(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::Usize(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                    NumInfo::Isize(l) => {
                        *l = NumLimit::Vals(self.parse_nums(l_p));
                    }
                },
                _ => unreachable!(),
            },
            None => {}
        }

        self.type_table.add(TypeInfo::Num(num_info))
    }

    //    fn parse_range<E: Debug, T: FromStr<Err=E>>(&mut self, p: Pair<Rule>) -> Range<T> {
    //        let mut p = p.into_inner();
    //        self.parse_num(p.next().unwrap())..self.parse_num(p.next().unwrap())
    //    }

    fn parse_nums<T: Num>(&mut self, p: Pair<Rule>) -> Vec<T>
        where
            <T as num_traits::Num>::FromStrRadixErr: std::fmt::Debug,
    {
        p.into_inner().map(|p| self.parse_num(p)).collect()
    }

    fn parse_str_type(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let c_style = p.next().unwrap().as_rule() == Rule::Cstr;
        let limit = match p.next() {
            Some(p) => Some(self.parse_strs(p)),
            None => None,
        };
        self.type_table.add(TypeInfo::str_info(c_style, limit))
    }

    fn parse_strs(&mut self, p: Pair<Rule>) -> Vec<String> {
        p.into_inner().map(|p| self.parse_str_literal(p)).collect()
    }

    fn parse_str_literal(&self, p: Pair<Rule>) -> String {
        p.into_inner().next().unwrap().as_str().into()
    }

    fn parse_slice(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let exp_p = p.next().unwrap();
        assert_eq!(exp_p.as_rule(), Rule::TypeExp);
        let tid = self.parse_type_exp(exp_p);
        let mut limit: (isize, isize) = (-1, -1);

        if let Some(p) = p.next() {
            match p.as_rule() {
                Rule::NumLiteral => limit.0 = self.parse_num(p),
                Rule::Range => {
                    let range = self.parse_range(p);
                    limit.0 = range.start;
                    limit.1 = range.end;
                }
                _ => unreachable!(),
            }
        }

        self.type_table.add(TypeInfo::slice_info(tid, limit))
    }

    fn parse_range<T: Num>(&self, p: Pair<Rule>) -> Range<T>
        where
            <T as num_traits::Num>::FromStrRadixErr: std::fmt::Debug,
    {
        let mut p = p.into_inner();
        let l = self.parse_num::<T>(p.next().unwrap());
        let h = self.parse_num::<T>(p.next().unwrap());
        l..h
    }

    fn parse_union(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let ident_p = p.next().unwrap();
        let fields_p = p.next().unwrap();
        assert_eq!(fields_p.as_rule(), Rule::Fields);
        let t_info = TypeInfo::union_info(ident_p.as_str(), self.parse_fields(fields_p));
        self.type_table.add(t_info)
    }

    fn parse_flag(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let ident = p.next().unwrap().as_str();
        let t_info = TypeInfo::flag_info(ident, self.parse_flag_members(p.next().unwrap()));
        self.type_table.add(t_info)
    }

    fn parse_flag_members(&mut self, p: Pair<Rule>) -> Vec<Flag> {
        p.into_inner().map(|p| self.parse_flag_member(p)).collect()
    }

    fn parse_flag_member(&mut self, p: Pair<Rule>) -> Flag {
        let mut p = p.into_inner();
        let ident = p.next().unwrap().as_str();
        // let val: i32 = self.parse_num(p.next().unwrap());
        let val = self.parse_num::<i32>(p.next().unwrap());
        Flag::new(ident, val)
    }

    fn parse_alias(&mut self, p: Pair<Rule>) -> TypeId {
        let mut p = p.into_inner();
        let ident_p = p.next().unwrap();
        let exp_p = p.next().unwrap();
        let tid = self.parse_type_exp(exp_p);
        let t_info = TypeInfo::alias_info(ident_p.as_str(), tid);
        self.type_table.add(t_info)
    }

    fn parse_num<T: Num>(&self, p: Pair<Rule>) -> T
        where
            <T as num_traits::Num>::FromStrRadixErr: std::fmt::Debug,
    {
        num::parse(p.as_str()).unwrap()
    }
    #[allow(dead_code)]
    fn report<E: Error>(_e: pest::error::Error<Rule>) -> E {
        todo!()
    }

    fn next_fid(&mut self) -> FnId {
        let id = self.fid_count;
        self.fid_count += 1;
        id
    }
}

struct GroupTable {
    groups: HashMap<GroupId, Group>,
    idents: HashMap<String, GroupId>,
}

impl Default for GroupTable {
    fn default() -> Self {
        GroupTable {
            groups: hashmap! {0 => Default::default()},
            idents: hashmap! {"Default".into() => 0},
        }
    }
}

impl GroupTable {
    pub fn id_of(&mut self, ident: &str) -> GroupId {
        if let Some(&gid) = self.idents.get(ident) {
            gid
        } else {
            let gid = self.next_id();
            let group = Group::new(gid, ident);
            self.groups.insert(gid, group);
            self.idents.insert(ident.into(), gid);
            gid
        }
    }

    pub fn get_mut(&mut self, id: GroupId) -> Option<&mut Group> {
        self.groups.get_mut(&id)
    }

    pub fn add_fn(&mut self, gid: GroupId, fn_info: FnInfo) {
        assert!(self.groups.contains_key(&gid));
        self.groups.get_mut(&gid).unwrap().fn_info(fn_info);
    }

    pub fn add_fns(&mut self, gid: GroupId, fns: impl IntoIterator<Item=FnInfo>) {
        assert!(self.groups.contains_key(&gid));
        self.groups.get_mut(&gid).unwrap().add_fns(fns);
    }

    fn next_id(&self) -> GroupId {
        self.groups.len() as GroupId
    }

    fn into_groups(self) -> Vec<Group> {
        self.groups.into_iter().map(|(_, g)| g).collect()
    }
}

/// Type record table
struct TypeTable {
    types: HashMap<TypeInfo, TypeId>,
    symbols: HashMap<String, TypeId>,
    unresolved: HashMap<String, TypeId>,
}

impl TypeTable {
    pub fn new() -> Self {
        TypeTable {
            types: HashMap::new(),
            symbols: HashMap::new(),
            unresolved: HashMap::new(),
        }
    }

    pub fn check(&self) -> Option<errors::Error> {
        if self.unresolved.is_empty() {
            None
        } else {
            // TODO Remove clone
            Some(errors::Error::from_idents(
                self.unresolved
                    .keys()
                    .map(|s| s.clone())
                    .collect::<Vec<_>>(),
            ))
        }
    }

    pub fn with_primitives() -> Self {
        let mut table = TypeTable::new();
        let types = TypeInfo::primitive_types();
        for (tid, t_info) in types.into_iter().enumerate() {
            table.types.insert(t_info, tid as TypeId);
        }
        table
    }

    fn next_tid(&self) -> TypeId {
        (self.unresolved.len() + self.types.len()) as TypeId
    }

    pub fn add(&mut self, t_info: TypeInfo) -> TypeId {
        if self.types.contains_key(&t_info) {
            *self.types.get(&t_info).unwrap()
        } else if t_info.ident().is_some() {
            let ident = t_info.ident().unwrap();
            let (ident, tid) = if self.unresolved.contains_key(ident) {
                self.unresolved.remove_entry(ident).unwrap()
            } else if self.symbols.contains_key(ident) {
                use colored::*;
                eprintln!("{}:{}:{}", "Error".red(), "Conflict Ident", ident);
                exit(1);
            } else {
                (String::from(ident), self.next_tid())
            };
            self.types.insert(t_info, tid);
            self.symbols.insert(ident, tid);
            tid
        } else {
            let tid = self.next_tid();
            self.types.insert(t_info, tid);
            tid
        }
    }

    pub fn id_of(&mut self, symbol: &str) -> TypeId {
        if self.symbols.contains_key(symbol) {
            return *(self.symbols.get(symbol).unwrap()) as TypeId;
        }
        let tid = self.next_tid();
        self.unresolved.insert(String::from(symbol), tid);
        self.symbols.insert(String::from(symbol), tid);
        tid
    }

    pub fn into_types(self) -> Vec<Type> {
        self.types
            .into_iter()
            .map(|(info, tid)| Type { info, tid })
            .collect()
    }
}
