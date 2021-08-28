use anyhow::Context;
use healer_core::{target::Target, HashMap, HashSet};
use std::fmt::Write;
use std::hash::Hash;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs::{create_dir_all, write},
    hash::Hasher,
    path::PathBuf,
    sync::Mutex,
};
use syz_wrapper::{report::Report, repro::ReproInfo};

#[derive(Default)]
pub struct CrashManager {
    out_dir: PathBuf,
    white_list: HashSet<String>,
    raw_log_count: AtomicU64,
    reproducing: Mutex<HashSet<String>>,
    reports: Mutex<HashMap<String, Vec<Report>>>,
    repros: Mutex<HashMap<String, ReproInfo>>,
}

impl CrashManager {
    pub fn new(out_dir: PathBuf) -> Self {
        Self {
            out_dir,
            ..Self::default()
        }
    }

    pub fn with_whitelist(white_list: HashSet<String>, out_dir: PathBuf) -> Self {
        Self {
            out_dir,
            white_list,
            raw_log_count: AtomicU64::new(0),
            reproducing: Mutex::new(HashSet::new()),
            reports: Mutex::new(HashMap::new()),
            repros: Mutex::new(HashMap::new()),
        }
    }

    pub fn save_raw_log(&self, crash_log: &[u8]) -> anyhow::Result<bool> {
        let count = self.raw_log_count.fetch_add(1, Ordering::Relaxed);
        if count >= 1024 {
            return Ok(false);
        }
        let out_dir = self.out_dir.join("crashes").join("raw_logs");
        if let Err(e) = create_dir_all(&out_dir) {
            if e.kind() != ErrorKind::AlreadyExists {
                return Err(e).context("failed to create raw_logs dir");
            }
        }
        let fname = out_dir.join(count.to_string());
        write(&fname, &crash_log).context("failed to wrtie raw log")?;
        Ok(true)
    }

    pub fn save_new_report(&self, target: &Target, report: Report) -> anyhow::Result<bool> {
        if self.white_list.contains(&report.title) {
            return Ok(false);
        }

        let title = report.title.clone();
        let mut id = None;
        {
            let mut reports = self.reports.lock().unwrap();
            let entry = reports
                .entry(title.clone())
                .or_insert_with(|| Vec::with_capacity(100));
            if entry.len() < 100 {
                entry.push(report.clone());
                id = Some(entry.len());
            }
        }
        if let Some(id) = id {
            self.save_report(target, report, id)?;
        }
        {
            let r = self.repros.lock().unwrap();
            if r.contains_key(&title) {
                return Ok(false);
            }
        }
        let mut ri = self.reproducing.lock().unwrap();
        Ok(ri.insert(title))
    }

    pub fn unique_crashes(&self) -> u64 {
        let raw = (self.raw_log_count.load(Ordering::Relaxed) != 0) as u64;
        let n = self.reports.lock().unwrap();
        n.len() as u64 + raw
    }

    fn save_report(&self, target: &Target, r: Report, id: usize) -> anyhow::Result<()> {
        let dir_name = Self::dir_name(&r.title);
        let out_dir = self.out_dir.join("crashes").join(&dir_name);
        if let Err(e) = create_dir_all(&out_dir) {
            if e.kind() != ErrorKind::AlreadyExists {
                return Err(e).context("failed to create report dir");
            }
        }
        if id == 1 {
            let mut meta = String::new();
            writeln!(meta, "TITLE: {}", r.title).unwrap();
            if let Some(cor) = r.corrupted.as_ref() {
                writeln!(meta, "CORRUPTED: {}", cor).unwrap();
            }
            if !r.to_mails.is_empty() {
                writeln!(meta, "MAINTAINERS (TO): {:?}", r.to_mails).unwrap();
            }
            if !r.cc_mails.is_empty() {
                writeln!(meta, "MAINTAINERS (CC): {:?}", r.cc_mails).unwrap();
            }
            write(out_dir.join("meta"), meta).context("failed to save report mate info")?;
        }
        write(
            out_dir.join(format!("prog{}", id)),
            r.prog.as_ref().unwrap().display(target).to_string(),
        )
        .context("failed to write report prog")?;
        write(out_dir.join(format!("log{}", id)), r.raw_log)
            .context("failed to write report log")?;
        write(out_dir.join(format!("report{}", id)), r.report).context("failed to write report")
    }

    pub fn repro_done(&self, title: &str, repro: Option<ReproInfo>) -> anyhow::Result<()> {
        {
            let mut ri = self.reproducing.lock().unwrap();
            ri.remove(title);
        }
        if let Some(repro) = repro {
            let mut save = false;
            {
                let mut repros = self.repros.lock().unwrap();
                if !repros.contains_key(title) {
                    repros.insert(title.to_string(), repro.clone());
                    save = true;
                }
            }
            if save {
                self.do_save_repro(title, repro)?;
            }
        }

        Ok(())
    }

    fn do_save_repro(&self, title: &str, repro: ReproInfo) -> anyhow::Result<()> {
        let out_dir = self.out_dir.join("crashes").join(Self::dir_name(title));
        let mut prog = format!("# {}\n\n", repro.opt);
        prog.push_str(&repro.p);
        let fname = out_dir.join("repro.prog");
        write(&fname, prog.as_bytes()).context("failed to write repro.prog")?;
        let fname = out_dir.join("run_history");
        write(&fname, repro.log.as_bytes()).context("failed to write run hitory")?;
        let fname = out_dir.join("repro.log");
        write(&fname, repro.repro_log.as_bytes()).context("failed to write repro log")?;

        if let Some(cprog) = repro.c_prog.as_ref() {
            let fname = out_dir.join("repro.c");
            write(&fname, cprog.as_bytes()).context("failed to write repro.c")?;
        }

        Ok(())
    }

    fn dir_name(title: &str) -> String {
        let mut dir_name = title.replace('/', "~");
        if dir_name.len() >= 255 {
            let mut hasher = ahash::AHasher::default();
            dir_name.hash(&mut hasher);
            let hash = hasher.finish();
            dir_name = format!("{:X}", hash);
        }
        dir_name
    }
}
