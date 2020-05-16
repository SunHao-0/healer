use fuzzer::{fuzz, prepare_env, show_info, Config};
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;
use tokio::fs::read_to_string;

#[derive(Debug, StructOpt)]
#[structopt(name = "fuzzer", about = "Kernel fuzzer of healer.")]
struct Settings {
    #[structopt(short = "c", long = "config", default_value = "healer-fuzzer.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    let settings = Settings::from_args();
    let cfg_data = read_to_string(&settings.config).await.unwrap_or_else(|e| {
        eprintln!(
            "Config file not found: {}: {}",
            settings.config.display(),
            e
        );
        exit(exitcode::IOERR)
    });

    let conf: Config = toml::from_str(&cfg_data).unwrap_or_else(|e| {
        eprintln!("Config Error:{}", e);
        exit(exitcode::CONFIG);
    });

    conf.check();
    show_info();
    prepare_env().await;
    fuzz(conf).await
}
