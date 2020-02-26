use fuzzer::{fuzz, prepare_env, Config};
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

const HEALER: &str = r"
 ___   ___   ______   ________   __       ______   ______
/__/\ /__/\ /_____/\ /_______/\ /_/\     /_____/\ /_____/\
\::\ \\  \ \\::::_\/_\::: _  \ \\:\ \    \::::_\/_\:::_ \ \
 \::\/_\ .\ \\:\/___/\\::(_)  \ \\:\ \    \:\/___/\\:(_) ) )_
  \:: ___::\ \\::___\/_\:: __  \ \\:\ \____\::___\/_\: __ `\ \
   \: \ \\::\ \\:\____/\\:.\ \  \ \\:\/___/\\:\____/\\ \ `\ \ \
    \__\/ \::\/ \_____\/ \__\/\__\/ \_____\/ \_____\/ \_\/ \_\/

";

#[tokio::main]
async fn main() {
    let settings = Settings::from_args();
    let conf: Config = toml::from_str(&read_to_string(settings.config).await.unwrap())
        .unwrap_or_else(|e| {
            eprintln!("Config Error:{}", e);
            exit(1);
        });

    println!("{}", HEALER);
    prepare_env().await;
    fuzz(conf).await
}
