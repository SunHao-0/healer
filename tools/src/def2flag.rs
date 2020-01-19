use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::process::{Command, Stdio};

use pest::iterators::Pair;
use pest::Parser;

use fots::types::TypeInfo;

/// Parse name of constant to fots's flag type
pub fn parse(defs: &str) -> Result<Vec<TypeInfo>, Error> {
    let items = ItemsParser::parse(defs)?;
    let flag_vals = run_cprog(&items.to_cprogs())?;
    parse_flag(items, flag_vals)
}

fn parse_flag(items: Items, mut vals: Vec<u8>) -> Result<Vec<TypeInfo>, Error> {
    assert!(!vals.is_empty());
    let mut result = Vec::new();
    vals.pop();
    let vals = String::from_utf8(vals).unwrap();

    for (flag, mut vals) in items.flags.into_iter().zip(vals.lines()) {
        let mut flags = Vec::new();
        vals = vals.trim();
        for (ident, val) in flag.members.into_iter().zip(vals.split_whitespace()) {
            flags.push(fots::types::Flag {
                ident,
                val: fots::num::parse::<i64>(val)?,
            });
        }
        result.push(TypeInfo::Flag {
            ident: flag.ident,
            flags,
        });
    }
    Ok(result)
}

// linux only
fn run_cprog(prog: &str) -> Result<Vec<u8>, Error> {
    let out_dir = std::env::temp_dir();

    let prog_file = out_dir.join("defs.c");
    std::fs::write(&prog_file, prog)?;

    let exe_file = out_dir.join("defs.out");

    let cc_out = Command::new("cc")
        .arg("-o")
        .arg(&exe_file)
        .arg(&prog_file)
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    if !cc_out.status.success() {
        return Err(Error::Def(String::from_utf8(cc_out.stderr).unwrap()));
    }

    let exe_out = Command::new(&exe_file)
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    if !exe_out.status.success() {
        panic!("Error exists in generated code");
    }
    Ok(exe_out.stdout)
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Parse: {0}")]
    Parse(#[from] pest::error::Error<Rule>),
    #[error("IO:{0}")]
    IO(#[from] std::io::Error),
    #[error("{0}")]
    Def(String),
    #[error("{0}")]
    ParseInt(#[from] std::num::ParseIntError),
}

#[derive(Parser)]
#[grammar = "defs.pest"]
pub struct Defs;

pub struct ItemsParser {
    include: HashSet<String>,
    flags: HashSet<Flag>,
}

impl Default for ItemsParser {
    fn default() -> Self {
        Self {
            include: HashSet::new(),
            flags: HashSet::new(),
        }
    }
}

impl ItemsParser {
    fn parse(text: &str) -> Result<Items, pest::error::Error<Rule>> {
        let parse_tree = Defs::parse(Rule::Root, text)?;

        let mut parser: ItemsParser = Default::default();
        for p in parse_tree {
            match p.as_rule() {
                Rule::Include => parser.parse_include(p),
                Rule::Flags => parser.parse_flags(p),
                Rule::EOI => break,
                _ => unreachable!(),
            }
        }
        Ok(parser.finish())
    }

    fn finish(self) -> Items {
        Items {
            includes: self.include.into_iter().collect(),
            flags: self.flags.into_iter().collect(),
        }
    }

    fn parse_include(&mut self, p: Pair<Rule>) {
        let mut ps = p.into_inner();
        let header = ps.next().unwrap().as_str();
        self.include.insert(header.into());
    }

    fn parse_flags(&mut self, p: Pair<Rule>) {
        use colored::*;
        let mut ps = p.into_inner();
        let ident = ps.next().unwrap().as_str();
        let mut const_symbols: HashSet<String> = HashSet::new();
        for p in ps {
            if !const_symbols.insert(p.as_str().into()) {
                eprintln!(
                    "{}: duplicate flag value: {} => {}",
                    "Warn".red(),
                    p.as_str(),
                    ident
                );
            }
        }

        if !self.flags.insert(Flag {
            ident: ident.into(),
            members: const_symbols.into_iter().collect(),
        }) {
            eprintln!("{}: duplicate flag: {}", "Warn".red(), ident);
        }
    }
}

struct Items {
    includes: Vec<String>,
    flags: Vec<Flag>,
}

impl Items {
    pub fn to_cprogs(&self) -> String {
        use std::fmt::Write;
        assert!(!self.flags.is_empty());
        let mut prog = String::new();
        for include in &self.includes {
            writeln!(prog, "#include<{}>", include).unwrap()
        }
        writeln!(prog, "\n#include<stdio.h>").unwrap();

        writeln!(prog, "int main(){{").unwrap();
        for flag in &self.flags {
            writeln!(prog, "\t//{}", flag.ident).unwrap();
            for member in &flag.members {
                writeln!(prog, "\tprintf(\"%#.8X \",{});", member).unwrap();
            }
            writeln!(prog, "\tprintf(\"\\n\");").unwrap();
        }
        writeln!(prog, "\treturn 0;}}").unwrap();
        prog
    }
}

struct Flag {
    ident: String,
    members: Vec<String>,
}

impl PartialEq for Flag {
    fn eq(&self, other: &Self) -> bool {
        self.ident.eq(&other.ident)
    }
}

impl Eq for Flag {}

impl Hash for Flag {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ident.hash(state)
    }
}
