//! Implementation of Healer-spec
//! Healer-spec is system call specification language of healer.
//! It is a domain specific language for kernel scenario.

#[allow(unused_variables)]
pub mod ast;
pub mod error;
pub mod parse;
mod util;

pub fn parse() {
    todo!()
}

use crate::error::*;
use crate::parse::*;
use crate::util::num::*;

pub fn parse_ident_test(ident: &str) -> Result<&str, ParseError> {
    let span = Span::new(ident);
    let ret = parse_ident(span);
    if let Ok((_, ident)) = ret {
        Ok(ident)
    } else if let Err(nom::Err::Error(e)) = ret {
        Err(e.filename("test.hl"))
    } else {
        unreachable!()
    }
}

pub fn parse_integer_test<T: Integer>(i: &str) -> Result<T, ParseError> {
    let span = Span::new(i);
    let ret = parse_integer(span);
    if let Err(nom::Err::Error(e)) = ret {
        Err(e.filename("test.hl"))
    } else if let Ok((_, i)) = ret {
        Ok(i)
    } else {
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hello_hlang() {
        assert!(true);
    }
}
