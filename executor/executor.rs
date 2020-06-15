use core::target::Target;
use executor::{exec_loop, Config};
use fots::types::Items;
use std::fs::{read, write};
use std::net::TcpStream;
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "healer-executor")]
pub struct Settings {
    /// Address of healer-fuzzer
    #[structopt(short = "a", long)]
    addr: String,
    /// Path of fots file
    #[structopt(short = "t", long)]
    target: String,

    #[structopt(short = "c", long)]
    concurrency: bool,

    #[structopt(short = "m", long = "memleak-check")]
    memleak_check: bool,
}

fn main() {
    let settings = Settings::from_args();

    let items = read(&settings.target).unwrap_or_else(|e| {
        eprintln!("Fail to read target:{}", e);
        exit(exitcode::NOINPUT);
    });
    let items: Items = bincode::deserialize(&items).unwrap_or_else(|e| {
        eprintln!("Fail to deserialize given target {}:{}", settings.target, e);
        exit(exitcode::DATAERR);
    });
    let target = Target::from(items);

    if settings.memleak_check {
        write("/sys/kernel/debug/kmemleak", "clear").unwrap();
    }

    let mut retry = 1;
    let conn = loop {
        match TcpStream::connect(&settings.addr) {
            Ok(c) => break c,
            Err(e) => {
                if retry == 5 {
                    eprintln!("Fail to connect to healer-fuzzer:{}", e);
                    exit(exitcode::NOHOST);
                }
                retry += 1;
                sleep(Duration::from_millis(100));
                continue;
            }
        }
    };

    let conf = Config {
        memleak_check: settings.memleak_check,
        concurrency: settings.concurrency,
    };

    exec_loop(target, conn, conf)
}
