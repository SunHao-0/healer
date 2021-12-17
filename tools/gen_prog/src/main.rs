use healer_core::gen;
use healer_core::relation::{Relation, RelationWrapper};
use healer_core::verbose::set_verbose;
use rand::prelude::*;
use rand::rngs::SmallRng;
use std::process::exit;
use structopt::StructOpt;
use syz_wrapper::sys::load_target;

#[derive(Debug, StructOpt)]
struct Settings {
    /// Target to inspect.
    #[structopt(long, default_value = "linux/amd64")]
    target: String,
    /// Number of progs to generate.
    #[structopt(long, short, default_value = "1")]
    n: usize,
    /// Verbose.
    #[structopt(long)]
    verbose: bool,
}

fn main() {
    let settings = Settings::from_args();
    env_logger::init();
    set_verbose(settings.verbose);
    let target = load_target(&settings.target).unwrap_or_else(|e| {
        eprintln!("failed to load target: {}", e);
        exit(1)
    });
    let relation = Relation::new(&target);
    let rw = RelationWrapper::new(relation);
    let mut rng = SmallRng::from_entropy();
    for _ in 0..settings.n {
        let p = gen::gen_prog(&target, &rw, &mut rng);
        println!("{}\n", p.display(&target));
    }
    exit(0);
}
