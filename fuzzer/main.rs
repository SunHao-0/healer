use std::{
    fs::{create_dir, write},
    path::{Path, PathBuf},
    process::{exit, id},
    time::{Duration, Instant},
    writeln,
};

use hl_fuzzer::gen::gen;
use hl_fuzzer::target::Target;
use hl_fuzzer::{
    bg_task::init_runtime,
    exec::{spawn_in_qemu, ExecConf, ExecOpt, ExecResult, QemuConf, SshConf},
    fuzz::ValuePool,
};
use rustc_hash::FxHashSet;
use std::env::args;

pub fn main() {
    let args = args().skip(1).collect::<Vec<_>>();

    let (sys, ty) = syscalls::syscalls();
    let target = Target::new(sys, ty);
    let pool = ValuePool::default();
    init_runtime();
    println!("[+] Target loaded.");

    let qemu_conf = QemuConf {
        img_path: PathBuf::from(&args[0]).into_boxed_path(),
        kernel_path: Some(PathBuf::from(&args[1]).into_boxed_path()),
        ..Default::default()
    };

    let ssh_conf = SshConf {
        ssh_key: PathBuf::from(&args[2]).into_boxed_path(),
        ..Default::default()
    };

    let exec_conf = ExecConf {
        executor: PathBuf::from(&args[3]).into_boxed_path(),
        ..Default::default()
    };

    let cpu_id: usize = args[4].parse().unwrap();

    println!("[+] Booting");
    let now = Instant::now();
    let mut handle = spawn_in_qemu(exec_conf, qemu_conf, ssh_conf, 1).unwrap_or_else(|e| {
        println!("{}", e);
        exit(1);
    });
    println!("[+] Boot finished, cost {}s", now.elapsed().as_secs());

    if let Err(e) = affinity::set_thread_affinity(&[cpu_id]) {
        eprintln!("[-] Failed to bind to cpu-{}: {}", cpu_id, e);
    }
    println!("[+] Bind to cpu-{}", cpu_id);

    let workdir = build_workdir();
    let exec_opt = ExecOpt::default();
    let mut brs: FxHashSet<u32> = FxHashSet::default();
    let mut success_cnt = 0;
    let mut failed_cnt = 0;
    let mut crash_cnt = 0;
    println!("[+] Let the fuzz begin!");

    let mut last_run = Instant::now();
    let log_duration = Duration::from_secs(10);

    loop {
        let p = gen(&target, &pool);
        // println!("{}", p);
        match handle.exec(&target, &p, exec_opt.clone()) {
            Ok(ret) => match ret {
                ExecResult::Normal(info) => {
                    for i in &info {
                        brs.extend(&i.branches);
                    }
                    success_cnt += 1;
                }
                ExecResult::Failed { info, err } => {
                    for i in &info {
                        brs.extend(&i.branches);
                    }
                    failed_cnt += 1;
                    println!("Failed: {}", err);
                }
                ExecResult::Crash(c) => {
                    use std::fmt::Write;
                    let stdout = String::from_utf8(c.qemu_stdout).unwrap_or_default();
                    let stderr = String::from_utf8(c.qemu_stderr).unwrap_or_default();
                    let mut log = String::new();
                    writeln!(log, "============== QEMU STDOUT ================").unwrap();
                    writeln!(log, "{}", stdout).unwrap();
                    writeln!(log, "============== QEMU STDERR ================").unwrap();
                    writeln!(log, "{}", stderr).unwrap();
                    writeln!(log, "============== SYZ LOG ================").unwrap();
                    writeln!(log, "{}", c.syz_out).unwrap();
                    // print first.
                    println!("{}", log);
                    println!("============== PROG ================");
                    println!("{}", c.syz_out);

                    let crash_dir = workdir.join(crash_cnt.to_string());
                    create_dir(&crash_dir).unwrap();
                    write(crash_dir.join("qemu-log"), &log).unwrap();
                    write(crash_dir.join("prog"), p.to_string()).unwrap();

                    crash_cnt += 1;
                }
            },
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
        if last_run.elapsed() > log_duration {
            println!(
                "br: {}, succ: {}, fail: {}, crash: {}",
                brs.len(),
                success_cnt,
                failed_cnt,
                crash_cnt
            );
            last_run = Instant::now();
        }
    }
}

fn build_workdir() -> Box<Path> {
    let d = PathBuf::from(format!("hl-workdir-{}", id()));
    create_dir(&d).unwrap_or_else(|e| {
        eprintln!("failed to create workdir {}: {}", d.display(), e);
    });
    d.into_boxed_path()
}
