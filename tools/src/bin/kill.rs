use nix::sys::signal::{kill, SIGINT};
use nix::unistd::Pid;
use std::env;
use std::process::exit;

fn main() {
    let fuzzer_pid = match env::var("HEALER_FUZZER_PID") {
        Ok(pid) => pid.parse::<i32>().unwrap_or_else(|_e| {
            eprintln!("Bad pid: HEALER_FUZZER_PID={}", pid);
            exit(exitcode::DATAERR)
        }),
        Err(_) => {
            let pid = env::args().nth(1).unwrap_or_else(|| {
                eprintln!("Can't find fuzzer pid by env var:HEALER_FUZZER_PID");
                exit(exitcode::NOINPUT);
            });
            pid.parse::<i32>().unwrap_or_else(|e| {
                eprintln!("Fail to parse pid {}: {}", pid, e);
                exit(exitcode::DATAERR)
            })
        }
    };

    kill(Pid::from_raw(fuzzer_pid), Some(SIGINT)).unwrap_or_else(|e| {
        eprintln!("Fail to kill {}:{}", fuzzer_pid, e);
    })
}
