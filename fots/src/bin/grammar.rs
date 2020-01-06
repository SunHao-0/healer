use std::env;
use std::fs::read_to_string;

use pest::iterators::Pair;

use fots::grammar::Rule;
use fots::parse_grammar;

fn show_pair(p: Pair<Rule>) {
    println!("Rule:{:?}, Str:{}", p.as_rule(), p.as_str());
    println!("Span:{:?}", p.as_span());
    for pair in p.into_inner() {
        show_pair(pair)
    }
}

fn main() {
    let input_file = env::args().skip(1).next().unwrap();
    let fots_content = read_to_string(input_file).unwrap();
    println!("File length:{};\n{:?}", fots_content.len(), fots_content);
    let parse_result = parse_grammar(&fots_content);
    let parse_result = parse_result.unwrap();
    for p in parse_result {
        show_pair(p);
    }
}
