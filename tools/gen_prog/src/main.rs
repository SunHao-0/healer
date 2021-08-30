use clap::{crate_authors, crate_description, crate_name, crate_version, AppSettings, Clap};
use healer_core::gen;
use healer_core::relation::{Relation, RelationWrapper};
use healer_core::verbose::set_verbose;
use rand::prelude::*;
use rand::rngs::SmallRng;
use std::process::exit;
use syz_wrapper::sys::load_target;

#[derive(Debug, Clap)]
#[clap(name=crate_name!(), author = crate_authors!(), about = crate_description!(), version = crate_version!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Settings {
    /// Target to inspect.
    #[clap(long, default_value = "linux/amd64")]
    target: String,
    /// Number of progs to generate.
    #[clap(long, short, default_value = "1")]
    n: usize,
    /// Verbose.
    #[clap(long)]
    verbose: bool,
}

fn main() {
    let settings = Settings::parse();
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
