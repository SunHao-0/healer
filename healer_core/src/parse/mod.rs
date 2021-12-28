//! Prog parse.
use crate::mutation::fixup;
use crate::prog::{Call, CallBuilder, Prog};
use crate::syscall::Syscall;
use crate::target::Target;
use crate::ty::{Dir, Type, TypeKind, TypeKind::*};
use crate::value::{
    DataValue, GroupValue, IntegerValue, PtrValue, ResValue, UnionValue, Value, VmaValue,
    ENCODING_ADDR_BASE,
};
use pest::{Parser, Span};
use std::fmt;

#[derive(Parser)]
#[grammar = "parse/prog_syntax.pest"]
pub struct SyntaxParser;

#[derive(Debug)]
pub struct ParseError<'a> {
    pub span: Option<Span<'a>>,
    pub kind: ParseErrorKind,
}

impl<'a> fmt::Display for ParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(span) = self.span.as_ref() {
            let (l, c) = span.start_pos().line_col();
            let mut s = span.as_str();
            let mut omitted = false;
            if s.len() > 32 {
                s = &s[..32];
                omitted = true;
            }
            write!(f, "{}:{} {}{}: ", l, c, s, if omitted { "..." } else { "" })?;
        }
        write!(f, "{}", self.kind)
    }
}

impl From<pest::error::Error<Rule>> for ParseError<'_> {
    fn from(e: pest::error::Error<Rule>) -> Self {
        Self {
            span: None, // pest already handled this,
            kind: ParseErrorKind::Syntax(e),
        }
    }
}

#[derive(Debug)]
pub enum ParseErrorKind {
    Syntax(pest::error::Error<Rule>),
    NoCalls,
    CallNotExists,
    PrototypeNotMatch,
    Value(Box<dyn std::error::Error>),
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => write!(f, "{}", e),
            Self::NoCalls => write!(f, "no calls"),
            Self::CallNotExists => write!(f, "call not exists"),
            Self::Value(e) => write!(f, "value: {}", e),
            Self::PrototypeNotMatch => write!(f, "prototype not match"),
        }
    }
}

/// Parse a prog
pub fn parse_prog<'a>(target: &Target, p: &'a str) -> Result<Prog, ParseError<'a>> {
    // get parsing tree first
    let call_nodes = SyntaxParser::parse(Rule::Prog, p)?;
    let mut calls = Vec::new();

    for node in call_nodes.filter(|node| node.as_rule() != Rule::EOI) {
        let call = convert_call(target, node)?;
        calls.push(call);
    }

    if calls.is_empty() {
        Err(ParseError {
            span: None,
            kind: ParseErrorKind::NoCalls,
        })
    } else {
        fixup(target, &mut calls);
        calls.shrink_to_fit();
        Ok(Prog::new(calls))
    }
}

// covert_XXX convert xxx node in the parsing tree to AST node.

type Node<'a> = pest::iterators::Pair<'a, Rule>;

fn convert_call<'a>(target: &Target, node: Node<'a>) -> Result<Call, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Call);

    let mut nodes = node.into_inner();
    let mut ret_res = None;

    let node = nodes.peek().unwrap(); // ret or name
    if node.as_rule() == Rule::Ret {
        let node = nodes.next().unwrap();
        let res_id = convert_res_name(node.into_inner().next().unwrap())?;
        ret_res = Some(res_id);
    }

    let syscall = convert_call_name(target, nodes.next().unwrap())?;

    let ret: Option<Value> = match (syscall.ret(), ret_res) {
        (Some(ty), Some(rid)) => Some(ResValue::new_res(ty.id(), rid, 0).into()),
        _ => None,
    };

    let mut args: Vec<Value> = Vec::with_capacity(syscall.params().len());
    let params = syscall.params();
    for (idx, node) in nodes.enumerate() {
        if idx >= params.len() {
            return Err(ParseError {
                span: Some(node.as_span()),
                kind: ParseErrorKind::PrototypeNotMatch,
            });
        }

        let ty = params[idx].ty();
        let dir = params[idx].dir().unwrap_or(Dir::In);
        let val = convert_value(target, ty, dir, node)?;
        args.push(val);
    }

    while args.len() < params.len() {
        let idx = args.len();
        let p = &params[idx];
        args.push(p.ty().default_value(p.dir().unwrap_or(Dir::In)));
    }

    let mut builder = CallBuilder::new(syscall.id());
    builder.args(args).ret(ret);
    Ok(builder.build())
}

#[inline]
fn convert_call_name<'a, 'b>(
    target: &'a Target,
    node: Node<'b>,
) -> Result<&'a Syscall, ParseError<'b>> {
    let name = node.as_str();
    target.syscall_of_name(name).ok_or_else(|| ParseError {
        span: Some(node.as_span()),
        kind: ParseErrorKind::CallNotExists,
    })
}

/// Parse one value
fn convert_value<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Value);

    let inner_node = node.into_inner().next().unwrap();
    match inner_node.as_rule() {
        Rule::Auto => convert_auto(target, ty, dir, inner_node),
        Rule::Int => convert_int(target, ty, dir, inner_node),
        Rule::Data => convert_data(target, ty, dir, inner_node),
        Rule::Ptr => convert_ptr(target, ty, dir, inner_node),
        Rule::Vma => convert_vma(target, ty, dir, inner_node),
        Rule::Res => convert_res(target, ty, dir, inner_node),
        Rule::Array => convert_array(target, ty, dir, inner_node),
        Rule::Struct => convert_struct(target, ty, dir, inner_node),
        Rule::Union => convert_union(target, ty, dir, inner_node),
        _ => unreachable!(),
    }
}

#[derive(Debug, Clone)]
pub struct TypeValueNotMatchError {
    rule: Rule,
    got: TypeKind,
}

impl TypeValueNotMatchError {
    pub fn new(rule: Rule, got: TypeKind) -> Self {
        Self { rule, got }
    }
}

impl fmt::Display for TypeValueNotMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "type, value not match, rule: {:?}, got: {:?}",
            self.rule, self.got
        )
    }
}

impl std::error::Error for TypeValueNotMatchError {}

fn convert_auto<'a>(
    _target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    if matches!(ty.kind(), Len | Csum) {
        Ok(IntegerValue::new(ty.id(), dir, 0).into())
    } else if let Some(const_ty) = ty.as_const() {
        Ok(IntegerValue::new(ty.id(), dir, const_ty.const_val()).into())
    } else {
        Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ))
    }
}

fn convert_int<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Int);
    let num = parse_int(node.clone())?;
    let val: Value = match ty.kind() {
        Res => ResValue::new_null(ty.id(), dir, num).into(),
        Const | Int | Flags | Len | Proc | Csum => IntegerValue::new(ty.id(), dir, num).into(),
        Vma => {
            let index = (-(num as i64) as usize) % target.special_ptrs().len();
            VmaValue::new_special(ty.id(), dir, index as u64).into()
        }
        Ptr => {
            let index = (-(num as i64) as usize) % target.special_ptrs().len();
            PtrValue::new_special(ty.id(), dir, index as u64).into()
        }
        _ => {
            return Err(value_error(
                node.as_span(),
                TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
            ))
        }
    };

    Ok(val)
}

fn convert_data<'a>(
    _target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Data);

    if !matches!(ty.kind(), BufferBlob | BufferString | BufferFilename) {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let mut nodes = node.into_inner(); // Data => {ReadableData | NonReadableData}
    let node = nodes.next().unwrap();
    let data_kind = node.as_rule();
    let mut nodes = node.into_inner();
    let data_node = nodes.next().unwrap();
    let span = data_node.as_span();
    let data = data_node.as_str().as_bytes();

    let mut data = match data_kind {
        Rule::ReadableData => decode_readable_data(span, data)?,
        Rule::NonReadableData => hex::decode(data).map_err(|e| value_error(span, e))?,
        _ => unreachable!(),
    };

    let mut size = data.len();
    if !ty.varlen() {
        size = ty.size() as usize;
    }
    if let Some(num_node) = nodes.next() {
        size = convert_num(num_node)? as usize;
    }
    if dir == Dir::Out {
        return Ok(DataValue::new_out_data(ty.id(), dir, size as u64).into());
    }
    while data.len() < size {
        data.push(0);
    }
    // if size != 0 {
    //     // keep input prog, even though the size is 0.
    //     unsafe { data.set_len(size) };
    // }

    Ok(DataValue::new(ty.id(), dir, data).into())
}

#[derive(Debug, Clone)]
pub struct BadEncodeError;

impl fmt::Display for BadEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad encoding")
    }
}

impl std::error::Error for BadEncodeError {}

#[inline]
fn consume<'a, 'b>(
    mut iter: impl Iterator<Item = &'b u8>,
    span: &Span<'a>,
) -> Result<u8, ParseError<'a>> {
    iter.next().copied().ok_or_else(|| ParseError {
        span: Some(span.clone()),
        kind: ParseErrorKind::Value(Box::new(BadEncodeError)),
    })
}

#[allow(clippy::while_let_loop)]
fn decode_readable_data<'a>(span: Span<'a>, data: &[u8]) -> Result<Vec<u8>, ParseError<'a>> {
    let mut data_iter = data.iter();
    let mut ret = Vec::with_capacity(data.len());

    loop {
        let v = if let Some(val) = data_iter.next() {
            *val
        } else {
            break;
        };

        if v != b'\\' {
            ret.push(v);
            continue;
        }
        let v = consume(&mut data_iter, &span)?;
        match v {
            b'x' => {
                let h = consume(&mut data_iter, &span)?;
                let l = consume(&mut data_iter, &span)?;
                let h = (h as char).to_digit(16).ok_or_else(|| ParseError {
                    span: Some(span.clone()),
                    kind: ParseErrorKind::Value(Box::new(BadEncodeError)),
                })?;
                let l = (l as char).to_digit(16).ok_or_else(|| ParseError {
                    span: Some(span.clone()),
                    kind: ParseErrorKind::Value(Box::new(BadEncodeError)),
                })?;
                let v = (h << 4) + l;
                ret.push(v as u8);
            }
            b'a' => ret.push(0x7),
            b'b' => ret.push(0x8),
            b'f' => ret.push(0xC),
            b'n' => ret.push(0xA),
            b'r' => ret.push(0xD),
            b't' => ret.push(0x9),
            b'v' => ret.push(0xB),
            b'\'' => ret.push(b'\''),
            b'"' => ret.push(b'"'),
            b'\\' => ret.push(b'\\'),
            _ => return Err(value_error(span, BadEncodeError)),
        }
    }

    Ok(ret)
}

fn convert_ptr<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Ptr);

    if !matches!(ty.kind(), Ptr) {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let ty = ty.checked_as_ptr();
    let elem_ty = target.ty_of(ty.elem());
    let elem_dir = ty.dir();
    let mut nodes = node.into_inner();

    let addr_node = nodes.next().unwrap();
    let addr = match addr_node.as_rule() {
        Rule::Int => parse_addr(addr_node)?,
        Rule::Auto => 0, // filled later
        _ => unreachable!(),
    };

    if nodes.peek().is_none() {
        // default value
        let elem_value = elem_ty.default_value(elem_dir);
        return Ok(PtrValue::new(ty.id(), dir, addr, elem_value).into());
    }

    let mut node = nodes.next().unwrap();
    if node.as_rule() == Rule::AnyPtr {
        node = nodes.next().unwrap(); // just ignore AnyPtr for now
    }
    let pointee = match node.as_rule() {
        Rule::Nil => None,
        Rule::Value => Some(Box::new(convert_value(target, elem_ty, elem_dir, node)?)),
        _ => unreachable!(),
    };

    let mut val = PtrValue::new_special(ty.id(), dir, 0);
    val.pointee = pointee;
    val.addr = addr;
    Ok(val.into())
}

fn convert_vma<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Vma);
    if ty.kind() != Vma {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let mut nodes = node.into_inner();
    let addr_node = nodes.next().unwrap();
    let size_node = nodes.next().unwrap();
    let mut addr = parse_addr(addr_node)?;
    let mut size = parse_int(size_node)?;
    addr -= addr % target.page_sz();
    size -= size % target.page_sz();
    if size == 0 {
        size = target.page_sz();
    }
    let max_mem = target.page_sz() * target.page_num();
    if size > max_mem {
        size = max_mem;
    }
    if addr > max_mem - size {
        addr = max_mem - size;
    }
    Ok(VmaValue::new(ty.id(), dir, addr, size).into())
}

#[derive(Debug, Clone)]
pub struct BadPtrAddrError;

impl fmt::Display for BadPtrAddrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad ptr addr value")
    }
}

impl std::error::Error for BadPtrAddrError {}

fn parse_addr(node: Node) -> Result<u64, ParseError> {
    let raw_addr = parse_int(node.clone())?;
    if raw_addr < ENCODING_ADDR_BASE {
        Err(value_error(node.as_span(), BadPtrAddrError))
    } else {
        Ok(raw_addr - ENCODING_ADDR_BASE)
    }
}

fn convert_res<'a>(
    _target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Res);
    if ty.kind() != Res {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let res_kind_node = node.into_inner().next().unwrap();
    let res_kind = res_kind_node.as_rule();
    let mut nodes = res_kind_node.into_inner();
    let val = match res_kind {
        Rule::OutRes => {
            let id = convert_res_name(nodes.next().unwrap())?;
            let mut v = 0;
            if let Some(node) = nodes.next() {
                v = parse_int(node)?;
            }
            ResValue::new_res(ty.id(), id, v)
        }
        Rule::InRes => {
            let id = convert_res_name(nodes.next().unwrap())?;
            let mut op_div = 0;
            if let Some(node) = nodes.next() {
                op_div = parse_int(node)?;
            }
            let mut op_add = 0;
            if let Some(node) = nodes.next() {
                op_add = parse_int(node)?;
            }
            let mut v = ResValue::new_ref(ty.id(), dir, id);
            v.op_add = op_add;
            v.op_div = op_div;
            v
        }
        _ => unreachable!(),
    };

    Ok(val.into())
}

fn convert_array<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Array);
    if ty.kind() != Array {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let ty = ty.checked_as_array();
    let elem_ty = ty.elem();
    let mut elems = Vec::new();
    for node in node.into_inner() {
        let elem = convert_value(target, elem_ty, dir, node)?;
        elems.push(elem);
    }
    if let Some(r) = ty.range() {
        if *r.start() == *r.end() {
            while elems.len() < *r.start() as usize {
                elems.push(elem_ty.default_value(dir));
            }
            elems.truncate(*r.start() as usize);
        }
    }

    Ok(GroupValue::new(ty.id(), dir, elems).into())
}

fn convert_struct<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Struct);
    if ty.kind() != Struct {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let ty = ty.checked_as_struct();
    let fields = ty.fields();
    let mut nodes = node.into_inner();
    let mut vals = Vec::new();
    let mut i = 0;
    while i < fields.len() {
        let f = &fields[i];
        let fty = f.ty();
        let fdir = f.dir().unwrap_or(dir);
        if let Some(const_ty) = fty.as_const() {
            if const_ty.pad() {
                vals.push(IntegerValue::new(fty.id(), fdir, 0).into());
                i += 1;
                continue;
            }
        }
        if let Some(val_node) = nodes.next() {
            let val = convert_value(target, fty, fdir, val_node)?;
            vals.push(val);
        } else {
            break;
        }
        i += 1;
    }

    while i < fields.len() {
        let f = &ty.fields()[i];
        let fty = f.ty();
        let fdir = f.dir().unwrap_or(dir);
        vals.push(fty.default_value(fdir));
        i += 1;
    }

    if let Some(extra_node) = nodes.next() {
        return Err(value_error(
            extra_node.as_span(),
            TypeValueNotMatchError::new(extra_node.as_rule(), Struct),
        ));
    }

    Ok(GroupValue::new(ty.id(), dir, vals).into())
}

#[derive(Debug, Clone)]
pub struct UnionOptionNotExists;

impl fmt::Display for UnionOptionNotExists {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "union option not exists")
    }
}

impl std::error::Error for UnionOptionNotExists {}

fn convert_union<'a>(
    target: &Target,
    ty: &Type,
    dir: Dir,
    node: Node<'a>,
) -> Result<Value, ParseError<'a>> {
    debug_assert_eq!(node.as_rule(), Rule::Union);
    if ty.kind() != Union {
        return Err(value_error(
            node.as_span(),
            TypeValueNotMatchError::new(node.as_rule(), ty.kind()),
        ));
    }

    let ty = ty.checked_as_union();
    let fields = ty.fields();
    let mut nodes = node.into_inner();
    let option_node = nodes.next().unwrap();
    let option = option_node.as_str();
    let idx = fields
        .iter()
        .position(|f| f.name() == option)
        .ok_or_else(|| value_error(option_node.as_span(), UnionOptionNotExists))?;

    let f = &fields[idx];
    let val = if let Some(val_node) = nodes.next() {
        convert_value(target, f.ty(), f.dir().unwrap_or(dir), val_node)?
    } else {
        f.ty().default_value(f.dir().unwrap_or(dir))
    };

    Ok(UnionValue::new(ty.id(), dir, idx as u64, val).into())
}

#[inline]
fn convert_res_name(node: Node) -> Result<u64, ParseError> {
    debug_assert_eq!(node.as_rule(), Rule::ResName);

    convert_num(node.into_inner().next().unwrap())
}

fn convert_num(node: Node) -> Result<u64, ParseError> {
    debug_assert_eq!(node.as_rule(), Rule::Number);

    let val_str = node.as_str();
    val_str.parse::<u64>().map_err(|e| ParseError {
        span: Some(node.as_span()),
        kind: ParseErrorKind::Value(Box::new(e)),
    })
}

#[inline]
fn parse_int(node: Node) -> Result<u64, ParseError> {
    let num_str = &node.as_str()[2..]; // skip '0x' | '0X'
    u64::from_str_radix(num_str, 16).map_err(|e| value_error(node.as_span(), e))
}

fn value_error<E: std::error::Error + 'static>(span: Span, e: E) -> ParseError {
    ParseError {
        span: Some(span),
        kind: ParseErrorKind::Value(Box::new(e)),
    }
}
