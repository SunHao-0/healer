use core::{analyze, c, gen};
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use tools::load_target;

#[derive(StructOpt, Debug)]
#[structopt(
    name = "gen",
    about = "Generate progs standard output",
    version = "0.2.0"
)]
struct Settings {
    /// Fots target
    #[structopt(long, short = "i", default_value = "syscalls")]
    items: PathBuf,

    /// Enable to translate generated program to stdout
    #[structopt(long, short = "t")]
    translate: bool,
}

fn main() {
    let settings = Settings::from_args();

    let target = load_target(&settings.items);
    let rt = analyze::static_analyze(&target);
    let p = gen::gen(&target, &rt, &Default::default());

    if settings.translate {
        let p = c::to_prog(&p, &target);
        println!("{}", p);
    } else {
        let p = bincode::serialize(&p).unwrap_or_else(|e| {
            eprintln!("Fail to serialize: {}", e);
            exit(exitcode::SOFTWARE)
        });

        stdout().write_all(&p).unwrap_or_else(|e| {
            eprintln!("Fail to Write: {}", e);
            exit(exitcode::IOERR)
        })
    }
}
