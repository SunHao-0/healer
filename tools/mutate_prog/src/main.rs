use clap::{crate_authors, crate_description, crate_name, crate_version, AppSettings, Clap};
use healer_core::corpus::CorpusWrapper;
use healer_core::gen::{self, set_prog_len_range, FAVORED_MAX_PROG_LEN, FAVORED_MIN_PROG_LEN};
use healer_core::mutation::mutate;
use healer_core::parse::parse_prog;
use healer_core::relation::Relation;
use healer_core::target::Target;
use healer_core::verbose::set_verbose;
use rand::prelude::*;
use rand::rngs::SmallRng;
use std::fs::read_to_string;
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
    /// Verbose.
    #[clap(long)]
    verbose: bool,
    /// Mutate `n` time.
    #[clap(long, short, default_value = "1")]
    n: usize,
    /// Prog to mutate, randomly generate if not given.
    #[clap(short, long)]
    prog: Option<PathBuf>,
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
    println!("mutating following prog:\n{}", p.display(&target));
    let corpus = dummy_corpus(&target, &relation, &mut rng);
    println!("corpus len: {}", corpus.len());
    set_verbose(settings.verbose);
    for _ in 0..settings.n {
        let mutated = mutate(&target, &relation, &corpus, &mut &mut rng, &mut p);
        if mutated {
            println!("mutated prog:\n{}", p.display(&target));
        }
    }
    exit(0); // no need to drop mem
}

fn dummy_corpus(target: &Target, relation: &Relation, rng: &mut SmallRng) -> CorpusWrapper {
    let corpus = CorpusWrapper::new();
    let n = rng.gen_range(8..=32);
    set_prog_len_range(3..8); // progs in corpus are always shorter
    for _ in 0..n {
        let prio = rng.gen_range(64..=1024);
        corpus.add_prog(gen::gen_prog(target, relation, rng), prio);
    }
    set_prog_len_range(FAVORED_MIN_PROG_LEN..FAVORED_MAX_PROG_LEN); // restore
    corpus
}
