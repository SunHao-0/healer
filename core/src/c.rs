/// Prog to c
///
/// This module translate internal prog representation to c script.
/// It does type mapping, varibles declarint ..
use crate::prog::{ArgIndex, ArgPos, Call, Prog};
use crate::target::Target;
use crate::value::Value;
use fots::types::{NumInfo, NumLimit, PtrDir, StrType, TypeId, TypeInfo};
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};

use std::fmt::Write;
use std::iter::repeat;

/// C lang Script
pub struct Script(Vec<Stmt>);

pub fn fmt_script(stmts: &Script, t: &Target) -> String {
    let mut result = String::new();

    for stmt in stmts.0.iter() {
        writeln!(result, "{};", fmt_stmt(stmt, t)).unwrap();
    }
    result
}

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
        let arg = translate_arg((call_index, ArgPos::Arg(arg_i)), v.tid, &v.val, t, s);
        args.push(arg);
    }

    let call = Exp::Call(CallExp {
        name: pt.call_name.clone(),
        args,
    });

    let stmt = if let Some(tid) = pt.r_tid {
        let var_name = s.var_names.next_r();
        s.res.insert((call_index, ArgPos::Ret), var_name.clone());
        Stmt::VarDecl {
            tid,
            var_name,
            val: Some(call),
        }
    } else {
        Stmt::SimpleExp(call)
    };
    s.stmts.push(stmt);
}

fn translate_arg(arg_index: ArgIndex, tid: TypeId, val: &Value, t: &Target, s: &mut State) -> Exp {
    match t.type_of(tid) {
        TypeInfo::Num(_) | TypeInfo::Flag { .. } | TypeInfo::Len { .. } => {
            Exp::NumLiteral(val.literal())
        }
        TypeInfo::Ptr { tid, dir, .. } => {
            if val == &Value::None {
                Exp::NULL
            } else {
                let var_name = decl_var(*tid, &val, t, s);
                if dir != &PtrDir::In && t.is_res(*tid) {
                    s.res.insert(arg_index, var_name.clone());
                }

                match t.type_of(*tid) {
                    TypeInfo::Str { .. } | TypeInfo::Slice { .. } => Exp::Var(var_name),
                    TypeInfo::Ptr { .. } => panic!("Multi-level ptr not support yet"),
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
        TypeInfo::Flag { .. } | TypeInfo::Num(_) => decl_num(tid, val, s),
        TypeInfo::Slice { tid: under_tid, .. } => decl_slice(tid, *under_tid, val, t, s),
        TypeInfo::Str { str_type, .. } => decl_str(tid, str_type.clone(), val, s),
        TypeInfo::Struct { .. } => decl_struct(tid, val, t, s),
        TypeInfo::Union { .. } => decl_union(tid, val, t, s),
        TypeInfo::Alias { tid, .. } | TypeInfo::Res { tid } => decl_var(*tid, val, t, s),
        TypeInfo::Len { tid, .. } => decl_num(*tid, val, s),

        TypeInfo::Ptr { .. } => panic!("Multi-level ptr not support yet"),
    }
}

fn decl_num(tid: TypeId, val: &Value, s: &mut State) -> String {
    let name = s.var_names.next_p("n");
    decl_def(tid, &name, Exp::NumLiteral(val.literal()), s);
    name
}

fn decl_slice(tid: TypeId, under_tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let name = s.var_names.next_p("a");
    let mut exps = Vec::new();
    match val {
        Value::Group(vals) => {
            for v in vals.iter() {
                exps.push(translate_arg((0, ArgPos::Ret), under_tid, v, t, s));
            }
        }
        _ => panic!("Value of type error"),
    };
    decl_def(tid, &name, Exp::ListExp(exps), s);

    name
}

fn decl_str(tid: TypeId, str_type: StrType, val: &Value, s: &mut State) -> String {
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
    decl_def(tid, &name, exp, s);
    name
}

fn decl_struct(tid: TypeId, val: &Value, t: &Target, s: &mut State) -> String {
    let ident = t.type_of(tid).ident().unwrap();
    let var_name = s.var_names.next_p(ident);
    decl(tid, &var_name, s);

    let vals = if let Value::Group(v) = val {
        v
    } else {
        panic!("Value of type error")
    };

    if let TypeInfo::Struct { fields, .. } = t.type_of(tid) {
        for (field, val) in fields.iter().zip(vals.iter()) {
            let selected_field = format!("{}.{}", var_name, field.ident);
            let exp = translate_arg((0, ArgPos::Ret), field.tid, val, t, s);
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
    decl(tid, &var_name, s);

    let (val, choice) = if let Value::Opt { choice, val } = val {
        (val, choice)
    } else {
        panic!("Value of type error")
    };

    if let TypeInfo::Union { fields, .. } = t.type_of(tid) {
        let field = &fields[*choice];
        let selected_field = format!("{}.{}", var_name, field.ident);
        let exp = translate_arg((0, ArgPos::Ret), field.tid, val, t, s);
        asign(selected_field, exp, s)
    } else {
        panic!("Type error")
    }
    var_name
}

fn decl(tid: TypeId, var_name: &str, s: &mut State) {
    s.stmts.push(Stmt::VarDecl {
        tid,
        var_name: var_name.to_string(),
        val: None,
    })
}

fn decl_def(tid: TypeId, var_name: &str, exp: Exp, s: &mut State) {
    s.stmts.push(Stmt::VarDecl {
        tid,
        var_name: var_name.to_string(),
        val: Some(exp),
    });
}

fn asign(var_name: String, exp: Exp, s: &mut State) {
    let stmt = Stmt::Asign { var_name, val: exp };
    s.stmts.push(stmt);
}

#[derive(Default)]
struct State {
    stmts: Vec<Stmt>,
    var_names: VarName,
    res: HashMap<ArgIndex, String>,
}

pub enum Stmt {
    VarDecl {
        tid: TypeId,
        var_name: String,
        val: Option<Exp>,
    },
    Asign {
        var_name: String,
        val: Exp,
    },
    SimpleExp(Exp),
}

pub fn fmt_stmt(stmt: &Stmt, t: &Target) -> String {
    match stmt {
        Stmt::VarDecl { tid, var_name, val } => {
            let mut var = var_name.clone();
            add_type(&mut var, *tid, t);
            if let Some(v) = val {
                format!("{}={}", var, v)
            } else {
                var
            }
        }
        Stmt::Asign { .. } | Stmt::SimpleExp(_) => stmt.to_string(),
    }
}

fn add_type(var: &mut String, tid: TypeId, t: &Target) {
    match t.type_of(tid) {
        TypeInfo::Num(l) => add_num(l, var),
        TypeInfo::Ptr { depth, tid, .. } => {
            add_ptr(var, *depth);
            add_type(var, *tid, t);
        }
        TypeInfo::Slice { tid, .. } => {
            add_slice(var);
            add_type(var, *tid, t);
        }
        TypeInfo::Str { .. } => add_str(var),
        TypeInfo::Struct { ident, .. } => add_name_type(var, ident, "struct"),
        TypeInfo::Union { ident, .. } => add_name_type(var, ident, "union"),
        TypeInfo::Flag { .. } => add_num(&NumInfo::I64(NumLimit::None), var),
        TypeInfo::Alias { tid, .. } => add_type(var, *tid, t),
        TypeInfo::Res { tid } => add_type(var, *tid, t),
        TypeInfo::Len { tid, .. } => add_type(var, *tid, t),
    }
}

fn add_name_type(var: &mut String, t_name: &str, t: &str) {
    var.insert(0, ' ');
    var.insert_str(0, t_name);

    var.insert(0, ' ');
    var.insert_str(0, t);
}

fn add_str(var: &mut String) {
    var.insert_str(0, "char *");
}

fn add_slice(var: &mut String) {
    var.push_str("[]");
}

fn add_ptr(var: &mut String, depth: usize) {
    assert_ne!(depth, 0);
    let stars: String = repeat('*').take(depth).collect();
    var.insert_str(0, &stars);
}

fn add_num(n: &NumInfo, var: &mut String) {
    match n {
        NumInfo::I8(_) => var.insert_str(0, "int8_t "),
        NumInfo::I16(_) => var.insert_str(0, "int16_t "),
        NumInfo::I32(_) => var.insert_str(0, "int32_t "),
        NumInfo::I64(_) => var.insert_str(0, "int "),
        NumInfo::U8(_) => var.insert_str(0, "uint8_t "),
        NumInfo::U16(_) => var.insert_str(0, "uint16_t "),
        NumInfo::U32(_) => var.insert_str(0, "uint32_t "),
        NumInfo::U64(_) => var.insert_str(0, "unsigned int "),
        NumInfo::Usize(_) => var.insert_str(0, "uintptr_t "),
        NumInfo::Isize(_) => var.insert_str(0, "intptr_t "),
    }
}

impl Display for Stmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Stmt::VarDecl { tid, var_name, val } => {
                write!(f, "{} {}", tid, var_name).unwrap();
                if let Some(v) = val {
                    write!(f, " = {}", v).unwrap();
                }
            }
            Stmt::Asign { var_name, val } => write!(f, "{} = {}", var_name, val).unwrap(),
            Stmt::SimpleExp(e) => write!(f, "{}", e).unwrap(),
        };
        Ok(())
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
