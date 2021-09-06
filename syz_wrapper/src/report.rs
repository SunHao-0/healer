use healer_core::prog::Prog;
use std::{env::temp_dir, fmt::Write, fs::write, path::PathBuf, process::Command};
use thiserror::Error;

#[derive(Default, Clone)]
pub struct Report {
    pub title: String,
    pub corrupted: Option<String>,
    pub to_mails: Vec<String>,
    pub cc_mails: Vec<String>,
    pub report: String,
    pub raw_log: Vec<u8>,
    pub prog: Option<Prog>,
}

#[derive(Clone)]
pub struct ReportConfig {
    pub os: String,
    pub arch: String,
    pub id: u64,
    pub syz_dir: String,
    pub kernel_obj_dir: Option<String>,
    pub kernel_src_dir: Option<String>,
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            id: 0,
            syz_dir: "./".to_string(),
            kernel_obj_dir: None,
            kernel_src_dir: None,
        }
    }
}

impl ReportConfig {
    pub fn check(&mut self) -> Result<(), String> {
        let syz_dir = PathBuf::from(&self.syz_dir);
        let syz_symbolize = syz_dir.join("bin").join("syz-symbolize");
        if !syz_symbolize.exists() {
            return Err(format!("{} not exists", syz_symbolize.display()));
        }
        if let Some(dir) = self.kernel_obj_dir.as_ref() {
            let dir = PathBuf::from(dir);
            if !dir.is_dir() {
                return Err(format!("{} not exists", dir.display()));
            }
        }
        if let Some(dir) = self.kernel_src_dir.as_ref() {
            let dir = PathBuf::from(dir);
            if !dir.is_dir() {
                return Err(format!("{} not exists", dir.display()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("syz_symbolize: {0}")]
    SyzSymbolize(String),
    #[error("parse: {0}")]
    Parse(String),
}

pub fn extract_report(
    config: &ReportConfig,
    p: &Prog,
    raw_log: &[u8],
) -> Result<Vec<Report>, ReportError> {
    let log_file = temp_dir().join(format!("healer-crash-log-{}.tmp", config.id));
    write(&log_file, raw_log)?;

    let syz_dir = PathBuf::from(&config.syz_dir);
    let mut syz_symbolize = Command::new(syz_dir.join("bin").join("syz-symbolize"));
    syz_symbolize
        .args(vec!["-os", &config.os])
        .args(vec!["-arch", &config.arch]);
    if let Some(kernel_obj) = config.kernel_obj_dir.as_ref() {
        syz_symbolize.arg("-kernel_obj").arg(kernel_obj);
    }
    if let Some(kernel_src) = config.kernel_src_dir.as_ref() {
        syz_symbolize.arg("-kernel_src").arg(kernel_src);
    }
    syz_symbolize.arg(&log_file);
    let output = syz_symbolize.output().unwrap();

    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout).into_owned();
        let mut ret = parse(&content);
        for r in ret.iter_mut() {
            r.prog = Some(p.clone());
            r.raw_log = Vec::from(raw_log);
        }
        if ret.is_empty() {
            Err(ReportError::Parse(content))
        } else {
            Ok(ret)
        }
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(ReportError::SyzSymbolize(err.into_owned()))
    }
}

fn parse(content: &str) -> Vec<Report> {
    let mut ret = Vec::new();
    let lines = &mut content.lines();
    let mut last_title = None;

    loop {
        let mut title = None;
        if last_title.is_none() {
            for l in &mut *lines {
                if l.contains("TITLE:") {
                    let l = l.trim();
                    title = Some(String::from(&l[7..]));
                    break;
                }
            }
        } else {
            title = last_title.take();
        }

        if title.is_none() {
            break;
        }

        let mut corrupted = None;
        for l in &mut *lines {
            if l.contains("CORRUPTED:") {
                if l.contains("true") {
                    let idx = l.find('(').unwrap();
                    let mut corr = String::from(&l[idx + 1..]);
                    corr.pop(); // drop ')'
                    corrupted = Some(corr);
                }
                break;
            }
        }

        let mut to_mails = Vec::new();
        for l in &mut *lines {
            if l.contains("MAINTAINERS (TO):") {
                let start = l.find('[').unwrap();
                let end = l.rfind(']').unwrap();
                if start + 1 != end {
                    for mail in l[start + 1..end].split_ascii_whitespace() {
                        to_mails.push(String::from(mail));
                    }
                }
                break;
            }
        }

        let mut cc_mails = Vec::new();
        for l in &mut *lines {
            if l.contains("MAINTAINERS (CC):") {
                let start = l.find('[').unwrap();
                let end = l.rfind(']').unwrap();
                if start + 1 != end {
                    for mail in l[start + 1..end].split_ascii_whitespace() {
                        cc_mails.push(String::from(mail));
                    }
                }
                break;
            }
        }

        let mut report = String::new();
        for l in &mut *lines {
            if l.contains("TITLE:") {
                let l = l.trim();
                last_title = Some(String::from(&l[7..]));
                break; // next report
            } else {
                writeln!(report, "{}", l).unwrap();
            }
        }

        ret.push(Report {
            title: title.unwrap(),
            corrupted,
            to_mails,
            cc_mails,
            report: report.trim().to_string(),
            ..Default::default()
        });
    }
    ret
}
