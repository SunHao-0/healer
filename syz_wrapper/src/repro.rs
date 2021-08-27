use crate::exec::{ExecOpt, FLAG_INJECT_FAULT};
use healer_core::{prog::Prog, target::Target};
use simd_json::json;
use std::env::temp_dir;
use std::fmt::Write;
use std::fs::write;
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ReproInfo {
    pub log: String,
    pub opt: String,
    pub p: String,
    pub c_prog: Option<String>,
    pub repro_log: String,
}

#[derive(Debug, Clone)]
pub enum ReproResult {
    Succ(ReproInfo),
    Failed(String),
}

#[derive(Debug, Error)]
pub enum ReproError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("syz_repro: {0}")]
    SyzRepro(String),
}

#[derive(Debug, Clone)]
pub struct ReproConfig {
    pub id: u64,
    pub target: String,
    pub syz_dir: String,
    pub work_dir: String,
    pub disk_img: String,
    pub kernel_img: String,
    pub ssh_key: String,
    pub qemu_count: usize,
    pub qemu_smp: usize,
    pub qemu_mem: usize,
}

impl Default for ReproConfig {
    fn default() -> Self {
        Self {
            id: 0,
            target: "linux/amd64".to_string(),
            syz_dir: "./".to_string(),
            work_dir: "./".to_string(),
            kernel_img: "./bzImage".to_string(),
            disk_img: "./stretch.img".to_string(),
            ssh_key: "./stretch.id_rsa".to_string(),
            qemu_count: 1,
            qemu_smp: 2,
            qemu_mem: 4096,
        }
    }
}

impl ReproConfig {
    pub fn check(&mut self) -> Result<(), String> {
        let syz_dir = PathBuf::from(&self.syz_dir);
        let syz_repro = syz_dir.join("bin").join("syz-repro");
        if !syz_repro.exists() {
            Err(format!("{} not exists", syz_repro.display()))
        } else {
            Ok(())
        }
    }
}

pub fn repro(
    config: &ReproConfig,
    target: &Target,
    crash_log: &[u8],
    run_history: &[(ExecOpt, Prog)],
) -> Result<ReproResult, ReproError> {
    let log = build_log(target, run_history, crash_log);
    let tmp_log = temp_dir().join(format!("healer-run_log-{}.tmp", config.id));
    write(&tmp_log, &log)?;
    let syz_conf = syz_conf(config);
    let tmp_conf = temp_dir().join(format!("healer-syz_conf-{}.tmp", config.id));
    write(&tmp_conf, syz_conf.as_bytes())?;
    let syz_dir = PathBuf::from(&config.syz_dir);
    let syz_repro = Command::new(syz_dir.join("bin").join("syz-repro"))
        .arg("-config")
        .arg(&tmp_conf)
        .arg(&tmp_log)
        .output()?;
    if syz_repro.status.success() {
        let log = String::from_utf8_lossy(&log).into_owned();
        let repro_log = String::from_utf8_lossy(&syz_repro.stdout).into_owned();
        Ok(parse_repro_log(log, repro_log))
    } else {
        let err = String::from_utf8_lossy(&syz_repro.stderr).into_owned();
        Err(ReproError::SyzRepro(err))
    }
}

#[allow(clippy::while_let_on_iterator)]
fn parse_repro_log(log: String, repro_log: String) -> ReproResult {
    const FAILED: &str = "reproduction failed:";
    let mut lines = repro_log.lines();
    while let Some(l) = lines.next() {
        if let Some(mut i) = l.rfind(FAILED) {
            i += FAILED.len();
            return ReproResult::Failed(String::from(l[i..].trim()));
        }

        if l.contains("opts: {") && l.contains("} crepro: ") {
            let mut opt_i = l.find("opts: ").unwrap();
            opt_i += "opts ".len();
            let mut repro_i = l.rfind("crepro: ").unwrap();
            let opt = String::from(l[opt_i..repro_i].trim());
            repro_i += "crepro:".len();
            let crepro = String::from(l[repro_i..].trim());
            let has_crepro = if crepro == "true" {
                true
            } else if crepro == "false" {
                false
            } else {
                continue; // bad line
            };

            if let Some(sp) = lines.next() {
                if !sp.is_empty() {
                    continue;
                }
            } else {
                break;
            }

            let mut p = String::new();
            while let Some(l) = lines.next() {
                if l.is_empty() {
                    break;
                }
                writeln!(p, "{}", l).unwrap();
            }
            if p.is_empty() {
                break;
            }

            let c_prog = if has_crepro {
                let p = lines.map(|l| format!("{}\n", l)).collect::<String>();
                Some(p)
            } else {
                None
            };

            return ReproResult::Succ(ReproInfo {
                log,
                opt,
                p,
                c_prog,
                repro_log,
            });
        }
    }
    ReproResult::Failed(format!("failed to repro:\n {}", log))
}

fn build_log(target: &Target, history: &[(ExecOpt, Prog)], crash_log: &[u8]) -> Vec<u8> {
    let mut progs = String::new();
    for (opt, p) in history.iter() {
        let mut opt_str = String::new();
        if opt.flags & FLAG_INJECT_FAULT != 0 {
            opt_str = format!(
                " (fault-call:{} fault-nth:{})",
                opt.fault_call, opt.fault_nth
            );
        }
        writeln!(progs, "executing program 0{}:", opt_str).unwrap();
        writeln!(progs, "{}", p.display(target)).unwrap();
    }

    let mut progs = progs.into_bytes();
    progs.extend(crash_log);
    progs
}

fn syz_conf(conf: &ReproConfig) -> String {
    let conf = json!({
        "target": conf.target,
        "http": "127.0.0.1:65534",
        "workdir": conf.work_dir,
        "image": conf.disk_img,
        "sshkey": conf.ssh_key,
        "syzkaller": conf.syz_dir,
        "procs": 2,
        "type": "qemu",
        "vm": {
            "count": conf.qemu_count,
            "kernel": conf.kernel_img,
            "cpu": conf.qemu_smp,
            "mem": conf.qemu_mem,
        }
    });

    conf.to_string()
}
