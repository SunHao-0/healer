use fuzzer::{fuzz, Config};
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use tokio::fs::read_to_string;

#[derive(Debug, StructOpt)]
#[structopt(name = "fuzzer", about = "Kernel fuzzer of healer.")]
struct Settings {
    #[structopt(short = "c", long, default_value = "healer-fuzzer.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    let settings = Settings::from_args();
    let conf: Config = toml::from_str(&read_to_string(settings.config).await.unwrap())
        .unwrap_or_else(|e| {
            eprintln!("Config Error:{}", e);
            exit(1);
        });

    fuzz(conf).await;
}
