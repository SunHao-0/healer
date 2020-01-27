use std::fs::{metadata, read_dir, read_to_string, write};
use std::path::PathBuf;
use std::process::exit;

use pest::iterators::Pairs;
use structopt::StructOpt;

use fots::grammar::Rule;
use fots::{parse_grammar, parse_items};

#[derive(Debug, StructOpt)]
#[structopt(about = "desciption language for healer", author = "sam")]
enum Settings {
    /// Build fots file
    Build {
        /// Show items after parse
        #[structopt(short, long)]
        v: bool,
        /// Out put file
        #[structopt(short = "o", long = "out")]
        out: Option<PathBuf>,
        /// Input files
        #[structopt(short = "f")]
        files: Option<Vec<PathBuf>>,
        /// Input directort
        #[structopt(short = "d")]
        dir: Option<PathBuf>,
    },
    /// Format fots files
    Format {
        /// Input files
        #[structopt(short = "f")]
        files: Option<Vec<PathBuf>>,
        /// Input directort
        #[structopt(short = "d")]
        dir: Option<PathBuf>,
    },
}

fn main() {
    let settings = Settings::from_args();
    match settings {
        Settings::Build { v, out, files, dir } => {
            let inputs = input_of(files, dir);
            let contents = read_all(&inputs);
            match parse_items(&contents) {
                Ok(items) => {
                    if v {
                        println!("{}", items);
                    }
                    if let Some(out) = out {
                        let items =
                            items.dump().unwrap_or_else(|e| err(&format!("{}", e)));
                        write(&out, items)
                            .unwrap_or_else(|e| err(&format!("{:?}:{}", out, e)))
                    }
                }
                Err(e) => err(&format!("{}", e)),
            }
        }
        Settings::Format { files, dir } => {
            let inputs = input_of(files, dir);
            for f in &inputs {
                let contents =
                    read_to_string(f).unwrap_or_else(|e| err(&format!("{:?}:{}", f, e)));
                match parse_grammar(&contents) {
                    Ok(ps) => {
                        let result = format(ps);
                        write(f, result)
                            .unwrap_or_else(|e| err(&format!("{:?}:{}", f, e)))
                    }
                    Err(e) => err(&format!("{}", e)),
                }
            }
        }
    }
}

fn input_of(files: Option<Vec<PathBuf>>, dir: Option<PathBuf>) -> Vec<PathBuf> {
    let inputs = if let Some(files) = files {
        return files.iter().map(|f| f.into()).collect();
    } else if let Some(dir) = dir {
        fots_files(dir)
    } else {
        fots_files(PathBuf::from("."))
    };
    if inputs.is_empty() {
        err("No input files");
    }
    let files = inputs
        .iter()
        .map(|p| format!("{:?}", p))
        .collect::<Vec<_>>()
        .join(",");
    info(&files);
    inputs
}

fn fots_files(dir: PathBuf) -> Vec<PathBuf> {
    match read_dir(&dir) {
        Ok(entries) => {
            let mut result = Vec::new();
            for e in entries.filter_map(|e| e.ok()) {
                if let Ok(f_type) = e.file_type() {
                    if f_type.is_file() {
                        match e.path().extension() {
                            Some(ex) if ex.eq("fots") => result.push(e.path()),
                            _ => continue,
                        }
                    }
                }
            }
            result
        }
        Err(e) => err(&format!("{:?}:{}", dir, e)),
    }
}

fn _show_pairs(ps: Pairs<Rule>) {
    let mut type_defs = Vec::new();
    let mut group_defs = Vec::new();
    let mut rule_def = Vec::new();
    let mut default_group = Vec::new();

    for p in ps {
        match p.as_rule() {
            Rule::TypeDef => type_defs.push(p),
            Rule::GroupDef => group_defs.push(p),
            Rule::RuleDef => rule_def.push(p),
            Rule::FuncDef => default_group.push(p),
            _ => unreachable!(),
        }
    }
}

fn format(ps: Pairs<Rule>) -> String {
    let result = String::new();
    for p in ps {
        match p.as_rule() {
            Rule::TypeDef => todo!(),
            Rule::GroupDef => todo!(),
            Rule::FuncDef => todo!(),
            Rule::RuleDef => todo!(),
            Rule::EOI => todo!(),
            _ => unreachable!(),
        }
    }
    result
}

fn _sizes(files: &[String]) -> Vec<u64> {
    let mut lens = Vec::with_capacity(files.len());

    for f in files {
        let meta = metadata(f).unwrap_or_else(|e| err(&format!("{}:{}", f, e)));
        lens.push(meta.len());
    }
    lens
}

fn read_all(files: &[PathBuf]) -> String {
    //    let lens = sizes(files);
    //    let mut buf = unsafe {
    //        let len = lens.iter().sum() as usize;
    //        let buf = Box::new_uninit_slice(len);
    //        Vec::from_raw_parts(std::mem::transmute(buf.as_ptr()), len, len)
    //    };
    files
        .iter()
        .map(|f| read_to_string(f).unwrap_or_else(|e| err(&format!("{:?}:{}", f, e))))
        .fold(String::new(), |a, b| a + &b)
}

fn err(msg: &str) -> ! {
    use colored::*;
    eprintln!("{}: {}", "Error".red(), msg);
    exit(1);
}

fn _warn(msg: &str) {
    use colored::*;
    eprintln!("{}: {}", "Warn".yellow(), msg);
}

fn info(msg: &str) {
    use colored::*;
    println!("{}: {}", "Info".blue(), msg);
}
