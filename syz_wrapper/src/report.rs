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
        let mut ret = parse(&output.stdout);
        for r in ret.iter_mut() {
            r.prog = Some(p.clone());
            r.raw_log = Vec::from(raw_log);
        }
        Ok(ret)
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(ReportError::SyzSymbolize(err.into_owned()))
    }
}

fn parse(content: &[u8]) -> Vec<Report> {
    let content = String::from_utf8_lossy(content);
    let mut ret = Vec::new();
    let mut lines = content.lines();

    loop {
        let title = parse_line(&mut lines, "TITLE:", |nl| String::from(&nl[7..]));
        if title.is_none() {
            break;
        }

        let corrupted = parse_line(&mut lines, "CORRUPTED:", |nl| {
            let mut corrupted = None;
            if nl.contains("true") {
                let idx = nl.find('(').unwrap();
                let mut corr = String::from(&nl[idx + 1..]);
                corr.pop(); // drop ')'
                corrupted = Some(corr);
            }
            corrupted
        });
        if corrupted.is_none() {
            break;
        }

        let to_mails = parse_line(&mut lines, "MAINTAINERS (TO):", |nl| {
            let start = nl.find('[').unwrap();
            let end = nl.rfind(']').unwrap();
            let mut mails = Vec::new();
            if start + 1 != end {
                for mail in nl[start + 1..end].split_ascii_whitespace() {
                    mails.push(String::from(mail));
                }
            }
            mails
        });
        if to_mails.is_none() {
            break;
        }

        let cc_mails = parse_line(&mut lines, "MAINTAINERS (CC):", |nl| {
            let start = nl.find('[').unwrap();
            let end = nl.rfind(']').unwrap();
            let mut mails = Vec::new();
            if start + 1 != end {
                for mail in nl[start + 1..end].split_ascii_whitespace() {
                    mails.push(String::from(mail));
                }
            }
            mails
        });
        if cc_mails.is_none() {
            break;
        }

        if lines.next().is_none() {
            // skip empty line.
            break;
        }

        let mut report = String::new();
        let mut first_empty = true;
        for l in &mut lines {
            if l.is_empty() {
                if first_empty {
                    first_empty = false;
                    continue;
                } else {
                    break;
                }
            }
            writeln!(report, "{}", l).unwrap();
        }

        ret.push(Report {
            title: title.unwrap(),
            corrupted: corrupted.unwrap(),
            to_mails: to_mails.unwrap(),
            cc_mails: cc_mails.unwrap(),
            report,
            ..Default::default()
        });
    }
    ret
}

fn parse_line<F, T>(lines: &mut std::str::Lines<'_>, val: &str, mut f: F) -> Option<T>
where
    F: FnMut(&str) -> T,
{
    for nl in lines {
        if nl.contains(val) {
            let nl = nl.trim();
            return Some(f(nl));
        }
    }
    None
}
