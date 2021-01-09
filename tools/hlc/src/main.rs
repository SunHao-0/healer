use hlc::parse;
use std::fs::{read_dir, read_to_string};
use std::{env::args, ffi::OsString, process::exit};

fn main() {
    let dir = args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: hlc dir-to-syzfiles");
        exit(1)
    });

    for entry in read_dir(&dir).unwrap().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension() == Some(&OsString::from("txt")) {
            let content = read_to_string(&p).unwrap();
            match parse(&content) {
                Ok(_r) => {
                    println!("[{}]: Ok", p.display());
                }
                Err(e) => {
                    eprintln!("{}", e.with_path(p.to_str().unwrap()));
                }
            }
        }
    }
}
