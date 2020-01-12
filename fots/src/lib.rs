//! Fots
//!
//! Fots is a fuzzing oriented type system used as system call description.
//! It has part of rust's type and type constructor with golang likely syntex.
//! A fots file consists four kind of items: type def, func def, group def and rule def.
//! Every func belongs to a group. A sample example as follow:
//! ``` fots
//! type fd = res<i32>
//! struct stat{...}
//! flag statx_flags { xx = 0x0 }
//! type statx_mask = u64{0x001,0x002}
//!
//! group FileStat{
//!     fn stat(file *In cstr, statbuf *Out stat)
//!     fn lstat(file *In cstr, statbuf *Out stat)
//!     fn fstat(fd Fd, statbuf *Out stat)
//!     fn newfstatat(dfd i32{0}, file *In cstr, statbuf *Out stat, f statx_flags)
//!     fn statx(fd Fd, file *In cstr, flags statx_flags, mask statx_mask, statxbuf *Out statx)
//! }
//}
//! ```
//!
#[macro_use]
extern crate maplit;
extern crate num_traits;
#[macro_use]
extern crate pest_derive;
#[macro_use]
extern crate prettytable;
#[cfg(test)]
#[macro_use]
extern crate runtime_fmt;
extern crate structopt;
#[macro_use]
extern crate thiserror;

use pest::iterators::Pairs;
use pest::Parser;

use grammar::{GrammarParser, Rule};

pub mod errors;
pub mod grammar;
pub mod items;
mod num;
pub mod types;

/// Parse plain text, return grammar structure of text.
///
/// Fots has defined its grammar and grammar parser, the result
/// of this method is a tree of rules which are used to parse text.
/// This should be useful if you want to build something like AST or
/// do some analysis.
///
/// ```
/// use fots::parse_grammar;
/// let text = "struct foo { arg1:i8, arg2:*[i8] }";
/// let mut pairs = parse_grammar(text).unwrap();   // pair is an iterator
/// assert_eq!(pairs.next().unwrap().as_str(),text);
/// ```
pub fn parse_grammar(text: &str) -> Result<Pairs<Rule>, pest::error::Error<Rule>> {
    GrammarParser::parse(Rule::Root, text)
}

/// Parse plain text, return items of text or error.
///
/// Fots file contains kinds of item: type definition ,function declaration,<br/>
/// group , rule. This method return these items.
/// ```
/// use fots::parse_items;
/// let text = "struct foo { arg1:i8, arg2:*[i8] }";
/// let mut re = parse_items(text);
/// assert!(re.is_ok());
/// ```
pub fn parse_items(text: &str) -> Result<types::Items, errors::Error> {
    items::parse(text)
}
