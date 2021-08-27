use std::sync::Mutex;

use healer_core::{HashMap, HashSet};
use syz_wrapper::{report::Report, repro::ReproInfo};

#[derive(Default)]
pub struct Crash {
    white_list: HashSet<String>,
    reproducing: Mutex<HashSet<String>>,
    reports: Mutex<HashMap<String, Vec<Report>>>,
    repros: Mutex<HashMap<String, ReproInfo>>,
}

impl Crash {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_whitelist(white_list: HashSet<String>) -> Self {
        Self {
            white_list,
            reproducing: Mutex::new(HashSet::new()),
            reports: Mutex::new(HashMap::new()),
            repros: Mutex::new(HashMap::new()),
        }
    }

    pub fn need_repro(&self, report: Report) -> bool {
        if self.white_list.contains(&report.title) {
            return false;
        }
        let title = report.title.clone();
        {
            let mut reports = self.reports.lock().unwrap();
            let entry = reports
                .entry(title.clone())
                .or_insert_with(|| Vec::with_capacity(100));
            if entry.len() < 100 {
                entry.push(report);
            }
        }
        {
            let r = self.repros.lock().unwrap();
            if r.contains_key(&title) {
                return false;
            }
        }
        let mut ri = self.reproducing.lock().unwrap();
        ri.insert(title)
    }

    pub fn record_repro(&self, title: String, repro: ReproInfo) {
        let mut repros = self.repros.lock().unwrap();
        repros.insert(title, repro);
    }
}
