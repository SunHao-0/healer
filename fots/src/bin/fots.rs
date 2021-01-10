use std::fs::{read_dir, read_to_string, write};
use std::path::PathBuf;
use std::process::exit;
use std::{ffi::OsString, fmt::Display};

use structopt::StructOpt;

use fots::parse_items;

#[derive(Debug, StructOpt)]
#[structopt(about = "system call desciption language", author = "SunHao")]
struct Settings {
    /// Show items after parsing.
    #[structopt(short = "v", long)]
    verbose: bool,
    /// Specify output file.
    #[structopt(short = "o", long)]
    out: Option<PathBuf>,
    /// Input files
    #[structopt(short = "f", long)]
    files: Option<Vec<PathBuf>>,
    /// Input directort
    #[structopt(short = "d", long, required_if("files", "None"))]
    dir: Option<PathBuf>,
    // TODO Format fots files
}

fn main() {
    let settings = Settings::from_args();
    let mut inputs = settings.files.unwrap_or_default();
    if let Some(d) = settings.dir {
        let dir = read_dir(d).unwrap_or_else(|e| err(format!("failed to read: {}", e)));
        for f in dir.filter_map(|f| f.ok()) {
            let file_path = f.path();
            if file_path.extension() == Some(&OsString::from("fots")) {
                inputs.push(file_path);
            }
        }
    }

    if inputs.is_empty() {
        err("no input file");
    }

    let mut contents = String::new();
    for f in inputs.into_iter() {
        let content = read_to_string(&f)
            .unwrap_or_else(|e| err(format!("failed to read {}: {}", f.display(), e)));
        contents.push_str(&content);
    }

    match parse_items(&contents) {
        Ok(items) => {
            if settings.verbose {
                println!("{}", items);
            }
            if let Some(out) = settings.out {
                let items = items.dump().unwrap_or_else(|e| err(e));
                write(&out, items).unwrap_or_else(|e| err(format!("{:?}:{}", out, e)))
            }
        }
        Err(e) => err(e),
    }
}

fn err<T: Display>(msg: T) -> ! {
    use colored::*;
    eprintln!("{}: {}", "Error".red(), msg);
    exit(1);
}
