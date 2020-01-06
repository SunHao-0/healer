use std::fs::read_to_string;

use fots::parse_items;

fn main() {
    let input_file = std::env::args().skip(1).next().unwrap();
    let content = read_to_string(&input_file).unwrap();
    let items = parse_items(&content).unwrap();
    println!("{}", items)
}
