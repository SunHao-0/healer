//! Prog parse.
use crate::mutation::fixup;
use crate::prog::{Call, CallBuilder, Prog};
use crate::syscall::Syscall;
use crate::target::Target;
use crate::value::{ResValue, Value};
use pest::{Parser, Span};
use std::fmt;

#[derive(Parser)]
#[grammar = "parse/prog_syntax.pest"]
pub struct SyntaxParser;

pub struct ParseError<'a> {
    pub span: Option<Span<'a>>,
    pub kind: ParseErrorKind,
}

impl<'a> fmt::Display for ParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(span) = self.span.as_ref() {
            let (l, c) = span.start_pos().line_col();
            write!(f, "in {}:{} {}: ", l, c, span.as_str())?;
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
    Value(Box<dyn std::error::Error>),
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => write!(f, "syntax: {}", e),
            Self::NoCalls => write!(f, "no calls"),
            Self::CallNotExists => write!(f, "call not exists"),
            Self::Value(e) => write!(f, "value: {}", e),
        }
    }
}

/// Parse a prog
pub fn parse_prog<'a>(target: &Target, p: &'a str) -> Result<Prog, ParseError<'a>> {
    // get parsing tree first
    let call_nodes = SyntaxParser::parse(Rule::Prog, p)?;
    let mut calls = Vec::new();

    for node in call_nodes {
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
        let res_id = convert_res_name(node)?;
        ret_res = Some(res_id);
    }

    let syscall = convert_call_name(target, nodes.next().unwrap())?;

    let ret: Option<Value> = match (syscall.ret(), ret_res) {
        (Some(ty), Some(rid)) => Some(ResValue::new_res(ty.id(), rid, 0).into()),
        _ => None,
    };

    let mut args: Vec<Value> = Vec::with_capacity(syscall.params().len());
    for node in nodes {
        // TODO check pad
        let val = convert_value(target, node)?;
        args.push(val);
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
fn convert_value<'a>(_target: &Target, _node: Node<'a>) -> Result<Value, ParseError<'a>> {
    todo!()
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
