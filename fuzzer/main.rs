use std::{
    path::PathBuf,
    process::exit,
    time::{Duration, Instant},
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

    let mut qemu_conf = QemuConf::default();
    qemu_conf.img_path = PathBuf::from(&args[0]).into_boxed_path();
    qemu_conf.kernel_path = Some(PathBuf::from(&args[1]).into_boxed_path());

    let mut ssh_conf = SshConf::default();
    ssh_conf.ssh_key = PathBuf::from(&args[2]).into_boxed_path();

    let mut exec_conf = ExecConf::default();
    exec_conf.executor = PathBuf::from(&args[3]).into_boxed_path();

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

    let exec_opt = ExecOpt::default();
    let mut bks: FxHashSet<u32> = FxHashSet::default();
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
                        bks.extend(&i.blocks);
                        brs.extend(&i.branches);
                    }
                    success_cnt += 1;
                }
                ExecResult::Failed { info, err } => {
                    for i in &info {
                        bks.extend(&i.blocks);
                        brs.extend(&i.branches);
                    }
                    failed_cnt += 1;
                    println!("Failed: {}", err);
                }
                ExecResult::Crash(c) => {
                    crash_cnt += 1;
                    let stdout = String::from_utf8(c.qemu_stdout).unwrap_or_default();
                    let stderr = String::from_utf8(c.qemu_stderr).unwrap_or_default();
                    println!("============== CRASHED ================");
                    println!("============== QEMU STDOUT ==============");
                    println!("{}", stdout);
                    println!("============== QEMU STDERR ==============");
                    println!("{}", stderr);
                    println!("============== SYZ STDERR ==============");
                    println!("{}", c.syz_out);
                    println!("============== PROG ==============");
                    println!("{}", p);
                }
            },
            Err(e) => {
                println!("Error: {}", e);
            }
        }
        if last_run.elapsed() > log_duration {
            println!(
                "bk: {}, br: {}, succ: {}, fail: {}, crash: {}",
                bks.len(),
                brs.len(),
                success_cnt,
                failed_cnt,
                crash_cnt
            );
            last_run = Instant::now();
        }
    }
}
