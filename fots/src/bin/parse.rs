use fots::parse::{Rule, Grammar};
use pest::iterators::Pair;
use pest::Parser;
use std::fs::read_to_string;
use std::env;

fn show_pair(p: Pair<Rule>) {
    println!("Rule:{:?}", p.as_rule());
    println!("Span:{:?}", p.as_span());
    for pair in p.into_inner() {
        show_pair(pair)
    }
}

fn main() {
    let input_file = env::args().skip(1).next().unwrap();
    let fots_content = read_to_string(input_file).unwrap();
    println!("File length:{};\n{:?}", fots_content.len(), fots_content);
    let parse_result = Grammar::parse(Rule::Root, &fots_content);
    let mut parse_result = parse_result.unwrap();
    show_pair(parse_result.next().unwrap());
}
