use std::fs::read_to_string;
use std::path::PathBuf;
use std::process::exit;

use structopt::StructOpt;

use fots::types::TypeInfo;
use tools::def2flag::parse;

#[derive(StructOpt)]
#[structopt(about = "defs to flags", author = "sam")]
struct Settings {
    #[structopt(short = "f", long)]
    files: Vec<PathBuf>,
    #[structopt(short = "o", long)]
    out: Option<PathBuf>,
}

fn main() {
    let settings = Settings::from_args();
    let mut buf = String::new();
    for f in &settings.files {
        buf += &read_to_string(f).unwrap_or_else(|e| {
            eprintln!("{:?}:{}", f, e);
            exit(1);
        });
    }
    if buf.is_empty() {
        exit(0);
    }
    match parse(&buf) {
        Ok(flags) => {
            use std::fmt::Write;
            let mut buf = String::new();
            for flag in &flags {
                writeln!(buf, "{}", flag).unwrap();
            }
            match settings.out {
                Some(path) => std::fs::write(&path, &buf).unwrap_or_else(|e| {
                    eprintln!("{:?}:{}", &path, e);
                    exit(1);
                }),
                None => println!("{}", buf),
            }
        }
        Err(e) => eprintln!("{}", e),
    };
}
