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

use pest::iterators::Pairs;
use pest::Parser;

use grammar::{GrammarParser, Rule};

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
pub fn parse_items(text: &str) -> Result<types::Items, pest::error::Error<Rule>> {
    items::parse(text)
}
