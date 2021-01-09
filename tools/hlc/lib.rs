#[macro_use]
extern crate pest_derive;
use pest::Parser;
use pest::{error::Error, iterators::Pairs};

pub mod constant;

#[derive(Parser)]
#[grammar = "../syz.pest"]
struct HlcParser;

pub fn parse(content: &str) -> Result<Pairs<Rule>, Error<Rule>> {
    HlcParser::parse(Rule::Root, content.as_ref())
}
