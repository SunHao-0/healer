use crate::{
    exec::syz::{ExecOpt, FLAG_INJECT_FAULT},
    fuzz::fuzzer::Fuzzer,
    model::Prog,
    Config,
};

use std::{env::temp_dir, fmt::Write, fs::write, process::Command};

use json::object;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct Repro {
    pub(crate) log: String,
    pub(crate) opt: String,
    pub(crate) p: String,
    pub(crate) c_prog: Option<String>,
    pub(crate) repro_log: String,
}

#[derive(Debug, Clone)]
pub enum ReproResult {
    Succ(Repro),
    Failed(String),
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("syz_repro: {0}")]
    SyzRepro(String),
}

impl Fuzzer {
    pub fn repro(&mut self, crash_log: &[u8]) -> Result<ReproResult, Error> {
        let history = self.run_history.make_contiguous();
        let log = build_log(history, crash_log);
        let tmp_log = temp_dir().join(format!("healer-run_log-{}.tmp", self.id));
        write(&tmp_log, &log)?;
        let syz_conf = syz_conf(&self.conf);
        let tmp_conf = temp_dir().join(format!("healer-syz_conf-{}.tmp", self.id));
        write(&tmp_conf, syz_conf.as_bytes())?;
        let syz_repro = Command::new(self.conf.syz_bin_dir.join("syz-repro"))
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
            Err(Error::SyzRepro(err))
        }
    }
}

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

            return ReproResult::Succ(Repro {
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

pub fn build_log(history: &[(ExecOpt, Prog)], crash_log: &[u8]) -> Vec<u8> {
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
        writeln!(progs, "{}", p.to_string()).unwrap();
    }
    let mut progs = progs.into_bytes();
    progs.extend(crash_log);
    progs
}

fn syz_conf(conf: &Config) -> String {
    let mut syz_dir = conf.syz_bin_dir.clone();
    syz_dir.pop(); // pop 'bin'
    let conf = object! {
        "target": conf.target.clone(),
        http: "127.0.0.1:65534",
        workdir: "./",
        image: conf.qemu_conf.disk_img.to_str().unwrap().to_string(),
        sshkey: conf.qemu_conf.ssh_key.to_str().unwrap().to_string(),
        syzkaller: syz_dir.to_str().unwrap().to_string(),
        procs: 2,
        "type": "qemu",
        vm:{
            count: 1,
            kernel: conf.qemu_conf.kernel_img.as_ref().map(|x| x.to_str().unwrap().to_string()).unwrap_or_default(),
            cpu: conf.qemu_conf.qemu_smp,
            mem: conf.qemu_conf.qemu_mem,
        }
    };

    conf.to_string()
}
