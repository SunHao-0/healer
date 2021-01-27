/// Temporary integration test.
use std::{
    env::args,
    fs::{create_dir, write},
    path::{Path, PathBuf},
    process::{exit, id},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Barrier, Mutex,
    },
    thread,
    time::{Duration, Instant},
    writeln,
};

use healer::{
    exec::{spawn_in_qemu, ExecConf, ExecOpt, ExecResult, QemuConf, SshConf},
    fuzz::fuzzer::ValuePool,
    gen::gen,
    targets::Target,
};
use rustc_hash::FxHashSet;

pub fn main() {
    let args = args().skip(1).collect::<Vec<_>>();
    env_logger::init();
    let target = Arc::new(Target::new("linux/amd64").unwrap());
    log::info!("Target loaded.");

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
    };
    let workdir = build_workdir();

    log::info!("Booting");
    let now = Instant::now();
    let brs: Arc<Mutex<FxHashSet<u32>>> = Arc::new(Mutex::new(FxHashSet::default()));
    let success_cnt = Arc::new(AtomicUsize::new(0));
    let failed_cnt = Arc::new(AtomicUsize::new(0));
    let crash_cnt = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(5));

    for i in 0..4 {
        let brs = Arc::clone(&brs);
        let success_cnt = Arc::clone(&success_cnt);
        let failed_cnt = Arc::clone(&failed_cnt);
        let crash_cnt = Arc::clone(&crash_cnt);
        let target = Arc::clone(&target);
        let workdir = workdir.clone();
        let barrier = Arc::clone(&barrier);
        let exec_conf = exec_conf.clone();
        let qemu_conf = qemu_conf.clone();
        let ssh_conf = ssh_conf.clone();
        thread::spawn(move || {
            let exec_opt = ExecOpt::default();
            let pool = ValuePool::default();
            let mut handle = spawn_in_qemu(exec_conf, qemu_conf, ssh_conf, i).unwrap_or_else(|e| {
                log::info!("{}", e);
                exit(1);
            });
            barrier.wait();
            loop {
                let p = gen(&target, &pool);
                match handle.exec(&exec_opt, &p) {
                    Ok(ret) => match ret {
                        ExecResult::Normal(info) => {
                            let mut brs = brs.lock().unwrap();
                            for i in &info {
                                brs.extend(&i.branches);
                            }
                            success_cnt.fetch_add(1, Ordering::Acquire);
                        }
                        ExecResult::Failed { info, err } => {
                            let mut brs = brs.lock().unwrap();
                            for i in &info {
                                brs.extend(&i.branches);
                            }
                            failed_cnt.fetch_add(1, Ordering::Acquire);
                            log::info!("Failed: {}", err);
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
                            log::info!("{}", log);
                            log::info!("============== PROG ================\n{}", p.to_string());

                            let crash_dir =
                                workdir.join(crash_cnt.load(Ordering::Acquire).to_string());
                            create_dir(&crash_dir).unwrap();
                            write(crash_dir.join("qemu-log"), &log).unwrap();
                            write(crash_dir.join("prog"), p.to_string()).unwrap();

                            crash_cnt.fetch_add(1, Ordering::Acquire);
                        }
                    },
                    Err(e) => {
                        log::warn!("Error: {}", e);
                    }
                }
            }
        });
    }

    barrier.wait();
    log::info!("Boot finished, cost {}s", now.elapsed().as_secs());
    log::info!("Let the fuzz begin!");
    let log_duration = Duration::from_secs(10);
    loop {
        thread::sleep(log_duration);
        let brs = brs.lock().unwrap();
        log::info!(
            "br: {}, succ: {}, fail: {}, crash: {}",
            brs.len(),
            success_cnt.load(Ordering::Acquire),
            failed_cnt.load(Ordering::Acquire),
            crash_cnt.load(Ordering::Acquire)
        );
    }
}

fn build_workdir() -> Box<Path> {
    let d = PathBuf::from(format!("hl-workdir-{}", id()));
    create_dir(&d).unwrap_or_else(|e| {
        log::warn!("failed to create workdir {}: {}", d.display(), e);
    });
    d.into_boxed_path()
}
