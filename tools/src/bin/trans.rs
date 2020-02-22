use core::c;
use core::prog::Prog;
use std::fs::read;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use tools::load_target;

#[derive(StructOpt, Debug)]
#[structopt(name = "Translate", about = "Tranlate progs to c")]
struct Settings {
    /// Fots target
    #[structopt(long, short = "p")]
    prog: PathBuf,
    #[structopt(long, short = "i")]
    items: PathBuf,
}

fn main() {
    let settings = Settings::from_args();

    let target = load_target(&settings.items);

    let p = read(&settings.prog).unwrap_or_else(|e| {
        eprintln!("Fail to read {:?}:{}", settings.prog, e);
        exit(exitcode::NOINPUT)
    });

    let p: Prog = bincode::deserialize(&p).unwrap_or_else(|e| {
        eprintln!("Fail to deserialize: {}", e);
        exit(exitcode::DATAERR)
    });

    let script = c::translate(&p, &target);

    println!("{}", script)
}
