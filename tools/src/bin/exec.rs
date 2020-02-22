use core::prog::Prog;
use executor::exec::exec;
use executor::exec::ExecResult;
use std::fs::read;
use std::io::stdout;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use tools::load_target;

#[derive(StructOpt, Debug)]
#[structopt(name = "Exec", about = "Execute prog directly")]
struct Settings {
    #[structopt(short = "p", long)]
    prog: PathBuf,
    #[structopt(short = "t", long)]
    items: PathBuf,
}

fn main() {
    let settings = Settings::from_args();
    let target = load_target(&settings.items);
    let p = read(&settings.prog).unwrap_or_else(|e| {
        eprintln!("Fail to read {:?}:{}", &settings.prog, e);
        exit(exitcode::NOINPUT)
    });

    let p: Prog = bincode::deserialize(&p).unwrap_or_else(|e| {
        eprintln!("Fail to deserialize {:?}:{}", &settings.prog, e);
        exit(exitcode::DATAERR)
    });

    exec(&p, &target, &mut stdout())
    // let len = p.len();
    // match fork_exec(p, &target) {
    //     ExecResult::Ok(covs) => {
    //         let mut total = 0;
    //         let mut each = Vec::new();
    //         for c in covs.iter() {
    //             total += c.len();
    //             each.push(c.len());
    //         }
    //
    //         println!("Prog len:{},Total pc:{},Executed:{:?}", len, total, each);
    //         exit(exitcode::OK)
    //     }
    //     ExecResult::Err(e) => {
    //         eprintln!("Error: {}", e);
    //         exit(exitcode::SOFTWARE)
    //     }
    // }
}
