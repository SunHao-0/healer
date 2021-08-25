use clap::{crate_authors, crate_description, crate_name, crate_version, AppSettings, Clap};
use healer_core::gen::{self, minimize};
use healer_core::parse::parse_prog;
use healer_core::relation::Relation;
use healer_core::verbose::set_verbose;
use rand::prelude::*;
use rand::rngs::SmallRng;
use std::fs::read_to_string;
use std::io::{self};
use std::path::PathBuf;
use std::process::exit;
use syz_wrapper::sys::load_target;

#[derive(Debug, Clap)]
#[clap(name=crate_name!(), author = crate_authors!(), about = crate_description!(), version = crate_version!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Settings {
    /// Target to inspect.
    #[clap(long, default_value = "linux/amd64")]
    target: String,
    /// Prog to mutate, randomly generate if not given.
    #[clap(short, long)]
    prog: Option<PathBuf>,
    /// Verbose.
    #[clap(long)]
    verbose: bool,
}

fn main() {
    let settings = Settings::parse();
    env_logger::builder().format_timestamp_secs().init();

    let target = load_target(&settings.target).unwrap_or_else(|e| {
        eprintln!("failed to load target: {}", e);
        exit(1)
    });
    let relation = Relation::new(&target);
    let mut rng = SmallRng::from_entropy();
    set_verbose(settings.verbose);
    let mut p = if let Some(prog_file) = settings.prog.as_ref() {
        let p_str = read_to_string(prog_file).unwrap_or_else(|e| {
            eprintln!("faile to read '{}': {}", prog_file.display(), e);
            exit(1)
        });
        parse_prog(&target, &p_str).unwrap_or_else(|e| {
            eprintln!("failed to parse: {} {}", prog_file.display(), e);
            exit(1)
        })
    } else {
        gen::gen_prog(&target, &relation, &mut rng)
    };
    println!(
        "> Prog to minimize, len {}:\n{}",
        p.calls().len(),
        p.display(&target)
    );
    let idx = read_line().trim().parse::<usize>().unwrap_or_else(|e| {
        eprintln!("invalid input: {}", e);
        exit(1)
    });

    minimize(&target, &mut p, idx, |p, idx| {
        println!("> current prog:\n{}", p.display(&target));
        println!("current idx: {}", idx);
        read_line().to_ascii_lowercase().trim() == "y"
    });

    println!("> prog after minimize:\n{}", p.display(&target));
    exit(0);
}

fn read_line() -> String {
    println!(">>>");
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap_or_else(|e| {
        eprintln!("failed to readline: {}", e);
        exit(1)
    });
    buf
}
