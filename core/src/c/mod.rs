/// Prog to c
///
/// This module translate internal prog representation to c script.
/// It does type mapping, varibles declarint ..
use crate::prog::{ArgIndex, ArgPos, Call, Prog};
use crate::target::Target;
use crate::value::Value;
use fots::types::{Field, NumInfo, NumLimit, PtrDir, StrType, TypeId, TypeInfo};
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};

use std::fmt::Write;

pub mod cths;

/// C Script
pub struct Script(pub Vec<Stmt>);

impl Display for Script {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        for s in self.0.iter() {
            writeln!(f, "{};", s).unwrap();
        }
        Ok(())
    }
}

pub fn to_script(p: &Prog, t: &Target) -> Script {
    let mut s: State = Default::default();

    // for each prototype and call
    for (i, c) in p.calls.iter().enumerate() {
        translate_call(i, c, t, &mut s);
    }
    Script(s.stmts)
}

pub fn to_prog(p: &Prog, t: &Target) -> String {
    use crate::c::cths::CTHS;

    let mut includes =
        hashset! {  "stddef.h".to_string(),"stdint.h".to_string(),"stdlib.h".to_string(),};
    let mut c_stmts = String::new();

    for (call_index, stmts) in iter_trans(p, t).enumerate() {
        let fn_info = t.fn_of(p.calls[call_index].fid);
        let call_name = fn_info.call_name.clone();
        if let Some(inc_attr) = fn_info.get_attr("inc") {
            if let Some(incs) = inc_attr.vals.as_ref() {
                for header in incs {
                    includes.insert(header.to_string());
                }
            }
        }
        if let Some(headers) = CTHS.get(&call_name as &str) {
            for header in headers {
                includes.insert((*header).to_string());
            }
        }

        writeln!(c_stmts, "{}", stmts.to_string()).unwrap();
    }

    let mut incs = String::new();
    writeln!(incs, "#define _GNU_SOURCE").unwrap();
    for header in includes.into_iter() {
        writeln!(incs, "#include<{}>", header).unwrap();
    }
    format!(
        r#"{}

int main(int argc, char **argv){{
{}
return 0;
}}"#,
        incs, c_stmts
    )
}

pub struct IterTranslate<'a> {
    p: &'a Prog,
    t: &'a Target,
    s: State,
    call_index: usize,
}

impl<'a> Iterator for IterTranslate<'a> {
    type Item = Script;

    fn next(&mut self) -> Option<Self::Item> {
        if self.call_index == self.p.len() {
            None
        } else {
            let c = &self.p.calls[self.call_index];
            translate_call(self.call_index, c, &self.t, &mut self.s);
            let mut stmts = Vec::new();
            std::mem::swap(&mut stmts, &mut self.s.stmts);
            self.call_index += 1;
            Some(Script(stmts))
        }
    }
}

pub fn iter_trans<'a>(p: &'a Prog, t: &'a Target) -> IterTranslate<'a> {
    IterTranslate {
        p,
        t,
        s: State::default(),
        call_index: 0,
    }
}

fn translate_call(call_index: usize, c: &Call, t: &Target, s: &mut State) {
    let pt = t.fn_of(c.fid);

    let mut args = Vec::new();
    for (arg_i, v) in c.args.iter().enumerate() {
        let arg_index = (call_index, ArgPos::Arg(arg_i));
        let arg = translate_arg(Some(arg_index), v.tid, &v.val, t, s);
        args.push(arg);
    }

    let call = Exp::Call(CallExp {
        name: pt.call_name.clone(),
        args,
    });

    if let Some(tid) = pt.r_tid {
        let var_name = s.var_names.next_r();
        s.res.insert((call_index, ArgPos::Ret), var_name.clone());

        let (ts, decl) = declarator_map(tid, &var_name, t);
        s.add_decl(ts, decl, Some(call));
    } else {
        s.stmts.push(Stmt::SimpleExp(call))
    };
}

fn translate_arg(
    arg_index: Option<ArgIndex>,
    tid: TypeId,
    val: &Value,
    t: &Target,
    s: &mut State,
) -> Exp {
    match t.type_of(tid) {
        TypeInfo::Num(_) | TypeInfo::Flag { .. } | TypeInfo::Len { .. } => {
            Exp::NumLiteral(val.literal())
        }
        TypeInfo::Ptr { tid, dir, depth } => {
            assert_eq!(*depth, 1, "Multi-level pointer not supported");

            if val == &Value::None {
                Exp::NULL
            } else {
                let var_name = decl_var(*tid, &val, t, s);
                if dir != &PtrDir::In && t.is_res(*tid) {
                    if let Some(index) = arg_index {
                        s.res.insert(index, var_name.clone());
                    }
                }
                if t.is_slice(*tid) || t.is_str(*tid) {
                    Exp::Var(var_name)
                } else {
                    Exp::Ref(var_name)
                }
            }
        }
        TypeInfo::Slice { .. } | TypeInfo::Str { .. } => {
            panic!("slice, str type can't be type param")
        }
        TypeInfo::Struct { .. } | TypeInfo::Union { .. } => Exp::Var(decl_var(tid, &val, t, s)),
        TypeInfo::Alias { tid, .. } => translate_arg(arg_index, *tid, val, t, s),
        TypeInfo::Res { tid } => {
            if let Value::Ref(index) = &val {
                Exp::Var(s.res[&index].clone())
            } else {
                translate_arg(arg_index, *tid, val, t, s)
            }
        }
    }
}

fn translate_slice(under_id: TypeId, val: &Value, t: &Target, s: &mut State) -> Exp {
    let vals = if let Value::Group(vals) = val {
        vals
    } else {
        panic!("Value type not match")
    };
    let exps = vals
        .iter()
        .map(|v| translate_arg(None, under_id, v, t, s))
        .collect();
    Exp::ListExp(exps)
}

fn translate_str(str_type: &StrType, v: &Value) -> Exp {
    let s = if let Value::Str(s) = v {
        s
    } else {
        panic!("Value type not match")
    };
    match str_type {
        StrType::Str => {
            let mut exps = Vec::new();
            for ch in s.chars() {
                exps.push(Exp::CharLiteral(ch));
            }
            Exp::ListExp(exps)
        }
        StrType::FileName | StrType::CStr => Exp::StrLiteral(s.clone()),
    }
}

/// declare varible of tid type with val value, record in state and return name of var
fn decl_var(tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    match t.type_of(tid) {
        TypeInfo::Num(info) => decl_num(info, val, s),
        TypeInfo::Flag { .. } => decl_num(&NumInfo::U32(NumLimit::None), val, s),
        TypeInfo::Len { tid, .. } => decl_var(*tid, val, t, s),
        TypeInfo::Str { str_type, .. } => decl_str(str_type, val, s),
        TypeInfo::Struct { ident, fields } => decl_struct(ident, fields, val, t, s),
        TypeInfo::Union { ident, fields } => decl_union(ident, fields, val, t, s),
        TypeInfo::Alias { tid, .. } => decl_var(*tid, val, t, s),
        TypeInfo::Res { tid } => decl_var(*tid, &Value::default_val(*tid, t), t, s),
        TypeInfo::Slice { tid: under_tid, .. } => {
            if let TypeInfo::Ptr { tid, .. } = t.type_of(*under_tid) {
                assert!(!t.is_slice(*tid), "Multi level slice not supported yet");
                // Multi level slice is useless
            }
            decl_slice(*under_tid, val, t, s)
        }
        TypeInfo::Ptr { .. } => unreachable!(),
    }
}

fn decl_num(num_info: &NumInfo, val: &Value, s: &mut State) -> String {
    let name = s.var_names.next_p("n");
    let (ts, decl) = map_num(num_info, &name);
    s.add_decl(ts, decl, Some(Exp::NumLiteral(val.literal())));
    name
}

fn decl_str(str_type: &StrType, val: &Value, s: &mut State) -> String {
    let name = s.var_names.next_p("s");
    let len = if let Value::Str(s) = val {
        s.len()
    } else {
        panic!("Value type not match")
    };

    let (ts, decl) = map_str(str_type, &name, Some(len));
    let exp = translate_str(str_type, val);
    s.add_decl(ts, decl, Some(exp));
    name
}

fn decl_slice(under_tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let name = s.var_names.next_p("a");
    let len = if let Value::Group(v) = val {
        v.len()
    } else {
        panic!("Value type not match")
    };

    let (ts, decl) = map_array(under_tid, &name, Some(len), t);
    let exp = translate_slice(under_tid, val, t, s);
    s.add_decl(ts, decl, Some(exp));
    name
}

fn decl_struct(ident: &str, fields: &[Field], val: &Value, t: &Target, s: &mut State) -> String {
    let var_name = s.var_names.next_p(ident);
    let (ts, decl) = map_struct(ident, &var_name);
    s.add_decl(ts, decl, None);

    let vals = if let Value::Group(v) = val {
        v
    } else {
        panic!("Value type not match")
    };

    for (field, val) in fields.iter().zip(vals.iter()) {
        let selected_field = format!("{}.{}", var_name, field.ident);
        let exp = translate_arg(None, field.tid, val, t, s);
        asign(selected_field, exp, s)
    }
    var_name
}

fn decl_union(ident: &str, fields: &[Field], val: &Value, t: &Target, s: &mut State) -> String {
    let var_name = s.var_names.next_p(ident);
    let (ts, decl) = map_union(ident, &var_name);
    s.add_decl(ts, decl, None);

    let (val, choice) = if let Value::Opt { choice, val } = val {
        (val, choice)
    } else {
        panic!("Value of type error")
    };
    let field = &fields[*choice];
    let selected_field = format!("{}.{}", var_name, field.ident);
    let exp = translate_arg(None, field.tid, val, t, s);
    asign(selected_field, exp, s);
    var_name
}

//fn decl(tid: TypeId, var_name: &str, init: Option<Exp>, t: &Target, s: &mut State) {
//    let (ts, decl) = declarator_map(tid, var_name, t);
//    let decl = Declaration {
//        ts,
//        init: InitDeclarator { decl, init },
//    };
//    s.stmts.push(Stmt::VarDecl(decl));
//}

fn declarator_map(tid: TypeId, var_name: &str, t: &Target) -> (TypeSpecifier, Declarator) {
    match t.type_of(tid) {
        TypeInfo::Num(n) => map_num(n, var_name),
        TypeInfo::Slice { tid, .. } => map_array(*tid, var_name, None, t),
        TypeInfo::Str { str_type, .. } => map_str(str_type, var_name, None),
        TypeInfo::Struct { ident, .. } => (
            TypeSpecifier::Struct(ident.clone()),
            Declarator::Ident(var_name.to_string()),
        ),
        TypeInfo::Union { ident, .. } => (
            TypeSpecifier::Union(ident.clone()),
            Declarator::Ident(var_name.to_string()),
        ),
        TypeInfo::Flag { .. } => map_flag(var_name),
        TypeInfo::Alias { tid, .. } => declarator_map(*tid, var_name, t),
        TypeInfo::Res { tid } => declarator_map(*tid, var_name, t),
        TypeInfo::Len { tid, .. } => declarator_map(*tid, var_name, t),

        TypeInfo::Ptr { tid, depth, .. } => {
            assert_eq!(*depth, 1);
            map_ptr(*tid, var_name, t)
        }
    }
}

fn map_struct(ident: &str, var_name: &str) -> (TypeSpecifier, Declarator) {
    (
        TypeSpecifier::Struct(ident.to_string()),
        Declarator::Ident(var_name.to_string()),
    )
}

fn map_union(ident: &str, var_name: &str) -> (TypeSpecifier, Declarator) {
    (
        TypeSpecifier::Union(ident.to_string()),
        Declarator::Ident(var_name.to_string()),
    )
}

fn map_flag(var_name: &str) -> (TypeSpecifier, Declarator) {
    (
        TypeSpecifier::Int32,
        Declarator::Ident(var_name.to_string()),
    )
}

fn map_ptr(tid: TypeId, var_name: &str, t: &Target) -> (TypeSpecifier, Declarator) {
    let (ts, decl) = declarator_map(tid, var_name, t);
    let decl = if t.is_str(tid) || t.is_slice(tid) {
        decl
    } else {
        Declarator::Ptr(Box::new(decl))
    };
    (ts, decl)
}

fn map_str(str_type: &StrType, var_name: &str, len: Option<usize>) -> (TypeSpecifier, Declarator) {
    let ts = TypeSpecifier::Char;
    let ident = Box::new(Declarator::Ident(var_name.to_string()));

    let decl = match str_type {
        StrType::Str => Declarator::Array {
            decl: ident,
            len: len.unwrap_or(0),
        },
        StrType::FileName | StrType::CStr => Declarator::Ptr(ident),
    };
    (ts, decl)
}

fn map_array(
    tid: TypeId,
    var_name: &str,
    len: Option<usize>,
    t: &Target,
) -> (TypeSpecifier, Declarator) {
    let (ts, decl) = declarator_map(tid, var_name, t);
    (
        ts,
        Declarator::Array {
            decl: Box::new(decl),
            len: len.unwrap_or(0),
        },
    )
}

fn map_num(n: &NumInfo, var_name: &str) -> (TypeSpecifier, Declarator) {
    let declarator = Declarator::Ident(var_name.to_string());
    let ts = match n {
        NumInfo::I8(_) => TypeSpecifier::Int8,
        NumInfo::I16(_) => TypeSpecifier::Int16,
        NumInfo::I32(_) => TypeSpecifier::Int32,
        NumInfo::I64(_) => TypeSpecifier::Int64,
        NumInfo::U8(_) => TypeSpecifier::Uint8,
        NumInfo::U16(_) => TypeSpecifier::Uint16,
        NumInfo::U32(_) => TypeSpecifier::Uint32,
        NumInfo::U64(_) => TypeSpecifier::Uint64,
        NumInfo::Usize(_) => TypeSpecifier::UintPtr,
        NumInfo::Isize(_) => TypeSpecifier::Intptr,
    };
    (ts, declarator)
}

fn asign(var_name: String, exp: Exp, s: &mut State) {
    let stmt = Stmt::Asign(Asignment {
        ident: var_name,
        init: exp,
    });
    s.stmts.push(stmt);
}

#[derive(Default)]
struct State {
    stmts: Vec<Stmt>,
    var_names: VarName,
    res: HashMap<ArgIndex, String>,
}

impl State {
    pub fn add_decl(&mut self, ts: TypeSpecifier, decl: Declarator, val: Option<Exp>) {
        self.stmts.push(Stmt::VarDecl(Declaration {
            ts,
            init: InitDeclarator { decl, init: val },
        }));
    }
}

#[derive(Clone)]
pub enum Stmt {
    VarDecl(Declaration),
    Asign(Asignment),
    SimpleExp(Exp),
}

impl Display for Stmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Stmt::VarDecl(d) => write!(f, "{}", d),
            Stmt::Asign(a) => write!(f, "{}", a),
            Stmt::SimpleExp(e) => write!(f, "{}", e),
        }
    }
}

#[derive(Clone)]
pub enum Exp {
    CharLiteral(char),
    NumLiteral(String),
    StrLiteral(String),
    ListExp(Vec<Exp>),
    Var(String),
    Ref(String),
    Call(CallExp),
    NULL,
}

impl Display for Exp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Exp::CharLiteral(ch) => write!(f, "'{}'", ch),
            Exp::NumLiteral(n) => write!(f, "{}", n),
            Exp::StrLiteral(s) => write!(f, "\"{}\"", s),
            Exp::ListExp(exps) => {
                let mut buf = String::new();
                buf.push('{');
                for exp in exps.iter() {
                    write!(buf, "{}", exp).unwrap();
                    buf.push(',');
                }
                if buf.ends_with(',') {
                    buf.pop();
                }
                buf.push('}');
                write!(f, "{}", buf)
            }
            Exp::Var(v) => write!(f, "{}", v),
            Exp::Ref(v) => write!(f, "&{}", v),
            Exp::Call(c) => write!(f, "{}", c),
            Exp::NULL => write!(f, "NULL"),
        }
    }
}

#[derive(Clone)]
pub struct CallExp {
    name: String,
    args: Vec<Exp>,
}

impl Display for CallExp {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut buf = String::new();
        write!(buf, "{}(", self.name).unwrap();
        for arg in self.args.iter() {
            write!(buf, "{},", arg).unwrap();
        }
        if buf.ends_with(',') {
            buf.pop();
        }
        buf.push(')');
        write!(f, "{}", buf)
    }
}

impl CallExp {
    pub fn new(name: String) -> Self {
        Self {
            name,
            args: Vec::new(),
        }
    }
}

struct VarName {
    param_count: HashMap<String, usize>,
    r_count: usize,
}

impl Default for VarName {
    fn default() -> Self {
        Self {
            param_count: HashMap::new(),
            r_count: 0,
        }
    }
}

impl VarName {
    pub fn next_p(&mut self, p_name: &str) -> String {
        let c = self.param_count.entry(p_name.to_string()).or_insert(0);
        let name = format!("{}_{}", p_name, c);
        *c += 1;
        name
    }

    pub fn next_r(&mut self) -> String {
        let name = format!("r{}", self.r_count);
        self.r_count += 1;
        name
    }
}

#[derive(Clone)]
pub struct Declaration {
    ts: TypeSpecifier,
    init: InitDeclarator,
}

impl Display for Declaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} {}", self.ts, self.init)
    }
}

#[derive(Clone)]
enum TypeSpecifier {
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    UintPtr,
    Int8,
    Int16,
    Int32,
    Int64,
    Intptr,
    Char,
    Struct(String),
    Union(String),
}

impl Display for TypeSpecifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        if let TypeSpecifier::Struct(ident) = self {
            write!(f, "struct {}", ident)
        } else if let TypeSpecifier::Union(ident) = self {
            write!(f, "union {}", ident)
        } else {
            let v = match self {
                TypeSpecifier::Uint8 => "uint8_t",
                TypeSpecifier::Uint16 => "uint16_t",
                TypeSpecifier::Uint32 => "uint32_t",
                TypeSpecifier::Uint64 => "uint64_t",
                TypeSpecifier::UintPtr => "uintptr_t",
                TypeSpecifier::Int8 => "int8_t",
                TypeSpecifier::Int16 => "int16_t",
                TypeSpecifier::Int32 => "int32_t",
                TypeSpecifier::Int64 => "int64_t",
                TypeSpecifier::Intptr => "intptr_t",
                TypeSpecifier::Char => "char",
                _ => unreachable!(),
            };
            write!(f, "{}", v)
        }
    }
}

#[derive(Clone)]
struct InitDeclarator {
    decl: Declarator,
    init: Option<Exp>,
}

impl Display for InitDeclarator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        if let Some(init) = self.init.as_ref() {
            write!(f, "{} = {}", self.decl, init)
        } else {
            write!(f, "{}", self.decl)
        }
    }
}

#[derive(Clone)]
enum Declarator {
    Ident(String),
    // only for str type
    Ptr(Box<Declarator>),
    Array { len: usize, decl: Box<Declarator> },
}

impl Declarator {
    fn under_declarator(&self) -> Option<&Declarator> {
        match self {
            Declarator::Ident(_) => None,
            Declarator::Array { decl, .. } => Some(decl),
            Declarator::Ptr(d) => Some(d),
        }
    }
}

impl Display for Declarator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let mut decl = self;
        let mut result = "$".to_string();

        loop {
            match decl {
                Declarator::Ident(ident) => {
                    result = result.replace('$', ident);
                    break;
                }
                Declarator::Ptr(_) => result = format!("*{}", result),
                Declarator::Array { len, .. } => {
                    result = format!("{}[{}]", result, len);
                }
            }
            decl = decl.under_declarator().unwrap();
        }
        write!(f, "{}", result)
    }
}

#[derive(Clone)]
pub struct Asignment {
    ident: String,
    init: Exp,
}

impl Display for Asignment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} = {}", self.ident, self.init)
    }
}
