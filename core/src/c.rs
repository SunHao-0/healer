/// Prog to c
///
/// This module translate internal prog representation to c script.
/// It does type mapping, varibles declarint ..
use crate::prog::{ArgIndex, ArgPos, Call, Prog};
use crate::target::Target;
use crate::value::Value;
use fots::types::{NumInfo, PtrDir, StrType, TypeId, TypeInfo};
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};

use std::fmt::Write;

/// C lang Script
pub struct Script(Vec<Stmt>);

impl Display for Script {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        for s in self.0.iter() {
            writeln!(f, "{};", s).unwrap();
        }
        Ok(())
    }
}

pub fn translate(p: &Prog, t: &Target) -> Script {
    let mut s: State = Default::default();

    // for each prototype and call
    for (i, c) in p.calls.iter().enumerate() {
        translate_call(i, c, t, &mut s);
    }
    Script(s.stmts)
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
        decl(tid, &var_name, Some(call), t, s);
    } else {
        s.stmts.push(Stmt::SimpleExp(call))
    };
}

fn translate_arg(arg_index: Option<ArgIndex>, tid: TypeId, val: &Value, t: &Target, s: &mut State) -> Exp {
    match t.type_of(tid) {
        TypeInfo::Num(_) | TypeInfo::Flag { .. } | TypeInfo::Len { .. } => {
            Exp::NumLiteral(val.literal())
        }
        TypeInfo::Ptr { tid, dir, .. } => {
            if let TypeInfo::Ptr { .. } = t.type_of(*tid) {
                panic!("Multi-level ptr not support yet")
            }

            if val == &Value::None {
                Exp::NULL
            } else {
                let var_name = decl_var(*tid, &val, t, s);
                if dir != &PtrDir::In && t.is_res(*tid) {
                    s.res.insert(arg_index.unwrap(), var_name.clone());
                }
                match t.type_of(*tid) {
                    TypeInfo::Str { .. } | TypeInfo::Slice { .. } => Exp::Var(var_name),
                    _ => Exp::Ref(var_name),
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

/// declare varible of tid type with val value, record in state and return name of var
fn decl_var(tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    match t.type_of(tid) {
        TypeInfo::Flag { .. } | TypeInfo::Num(_) => decl_num(tid, val, t, s),
        TypeInfo::Slice { tid: under_tid, .. } => decl_slice(tid, *under_tid, val, t, s),
        TypeInfo::Str { str_type, .. } => decl_str(tid, str_type.clone(), val, t, s),
        TypeInfo::Struct { .. } => decl_struct(tid, val, t, s),
        TypeInfo::Union { .. } => decl_union(tid, val, t, s),
        TypeInfo::Alias { tid, .. } | TypeInfo::Res { tid } => decl_var(*tid, val, t, s),
        TypeInfo::Len { tid, .. } => decl_num(*tid, val, t, s),

        TypeInfo::Ptr { depth, .. } => {
            assert_eq!(*depth, 1);
            panic!()
        }
    }
}

fn decl_num(tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let name = s.var_names.next_p("n");
    decl(tid, &name, Some(Exp::NumLiteral(val.literal())), t, s);
    name
}

fn decl_slice(tid: TypeId, under_tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let name = s.var_names.next_p("a");
    let mut exps = Vec::new();
    match val {
        Value::Group(vals) => {
            for v in vals.iter() {
                exps.push(translate_arg(None, under_tid, v, t, s));
            }
        }
        _ => panic!("Value of type error"),
    };
    decl(tid, &name, Some(Exp::ListExp(exps)), t, s);

    name
}

fn decl_str(tid: TypeId, str_type: StrType, val: &Value, t: &Target, s: &mut State) -> String {
    let exp = match str_type {
        StrType::Str => {
            let mut exps = Vec::new();
            if let Value::Str(s) = val {
                for c in s.chars() {
                    exps.push(Exp::CharLiteral(c));
                }
            } else {
                panic!("Value of type error")
            }
            Exp::ListExp(exps)
        }
        StrType::CStr | StrType::FileName => Exp::StrLiteral(val.literal()),
    };

    let name = s.var_names.next_p("s");
    decl(tid, &name, Some(exp), t, s);
    name
}

fn decl_struct(tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let ident = t.type_of(tid).ident().unwrap();
    let var_name = s.var_names.next_p(ident);
    decl(tid, &var_name, None, t, s);

    let vals = if let Value::Group(v) = val {
        v
    } else {
        panic!("Value of type error")
    };

    if let TypeInfo::Struct { fields, .. } = t.type_of(tid) {
        for (field, val) in fields.iter().zip(vals.iter()) {
            let selected_field = format!("{}.{}", var_name, field.ident);
            let exp = translate_arg(None, field.tid, val, t, s);
            asign(selected_field, exp, s)
        }
    } else {
        panic!("Type error")
    }

    var_name
}

fn decl_union(tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let ident = t.type_of(tid).ident().unwrap();
    let var_name = s.var_names.next_p(ident);
    decl(tid, &var_name, None, t, s);

    let (val, choice) = if let Value::Opt { choice, val } = val {
        (val, choice)
    } else {
        panic!("Value of type error")
    };

    if let TypeInfo::Union { fields, .. } = t.type_of(tid) {
        let field = &fields[*choice];
        let selected_field = format!("{}.{}", var_name, field.ident);
        let exp = translate_arg(None, field.tid, val, t, s);
        asign(selected_field, exp, s)
    } else {
        panic!("Type error")
    }
    var_name
}

fn decl(tid: TypeId, var_name: &str, init: Option<Exp>, t: &Target, s: &mut State) {
    let (ts, decl) = declarator_map(tid, var_name, t);
    let decl = Declaration {
        ts,
        init: InitDeclarator { decl, init },
    };
    s.stmts.push(Stmt::VarDecl(decl));
}

fn declarator_map(tid: TypeId, var_name: &str, t: &Target) -> (TypeSpecifier, Declarator) {
    match t.type_of(tid) {
        TypeInfo::Num(n) => map_num(n, var_name),
        TypeInfo::Slice { tid, .. } => map_array(*tid, var_name, t),
        TypeInfo::Str { str_type, .. } => map_str(str_type, var_name),
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

fn map_flag(var_name: &str) -> (TypeSpecifier, Declarator) {
    (
        TypeSpecifier::Int32,
        Declarator::Ident(var_name.to_string()),
    )
}

fn map_ptr(tid: TypeId, var_name: &str, t: &Target) -> (TypeSpecifier, Declarator) {
    let (ts, _) = declarator_map(tid, var_name, t);
    (ts, Declarator::Ptr(var_name.to_string()))
}

fn map_str(str_type: &StrType, var_name: &str) -> (TypeSpecifier, Declarator) {
    let ts = TypeSpecifier::Char;
    let ident = var_name.to_string();

    let decl = match str_type {
        StrType::Str => Declarator::Array(Box::new(Declarator::Ident(ident))),
        StrType::FileName | StrType::CStr => Declarator::Ptr(ident),
    };
    (ts, decl)
}

fn map_array(tid: TypeId, var_name: &str, t: &Target) -> (TypeSpecifier, Declarator) {
    let (ts, decl) = declarator_map(tid, var_name, t);
    (ts, Declarator::Array(Box::new(decl)))
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

pub struct Declaration {
    ts: TypeSpecifier,
    init: InitDeclarator,
}

impl Display for Declaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} {}", self.ts, self.init)
    }
}

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

enum Declarator {
    Ident(String),
    // only for str type
    Ptr(String),
    Array(Box<Declarator>),
}

impl Display for Declarator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Declarator::Ident(ident) => write!(f, "{}", ident),
            Declarator::Ptr(ident) => write!(f, "*{}", ident),
            Declarator::Array(d) => write!(f, "{}[]", d),
        }
    }
}

pub struct Asignment {
    ident: String,
    init: Exp,
}

impl Display for Asignment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{} = {}", self.ident, self.init)
    }
}
