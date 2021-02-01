use crate::{
    exec::{serialize, CallExecInfo, CrashInfo, ExecError, ExecHandle, ExecOpt, ExecResult},
    fuzz::{
        input::Input,
        mutation::{mutate_args, seq_reuse},
        queue::Queue,
        stats::*,
    },
    gen::gen,
    model::{Prog, ProgWrapper, SyscallRef, TypeRef, Value},
    targets::Target,
};

use std::{
    cmp::max,
    collections::VecDeque,
    env::temp_dir,
    fmt::Write,
    fs::{create_dir_all, write},
    hash::{Hash, Hasher},
    io::ErrorKind,
    iter::FromIterator,
    path::PathBuf,
    process::{exit, Command},
    sync::RwLock,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use rand::{thread_rng, Rng};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

/// Interesting values extracted inputs. not implemented yet.
pub type ValuePool = FxHashMap<TypeRef, VecDeque<Arc<Value>>>;

pub struct Fuzzer {
    // shared between different fuzzers.
    // TODO sync prog between diffierent threads.
    // progs: Arc<RwLock<FxHashMap<usize, Vec<ProgWrapper>>>>,
    pub(crate) max_cov: Arc<RwLock<FxHashSet<u32>>>,
    pub(crate) calibrated_cov: Arc<RwLock<FxHashSet<u32>>>,
    pub(crate) relations: Arc<RwLock<FxHashMap<SyscallRef, FxHashSet<SyscallRef>>>>,
    pub(crate) crashes: Arc<Mutex<FxHashMap<String, VecDeque<Report>>>>,
    pub(crate) raw_crashes: Arc<Mutex<VecDeque<Report>>>,
    pub(crate) stats: Arc<Stats>,

    // local data.
    pub(crate) id: u64,
    pub(crate) target: Target,
    pub(crate) local_vals: ValuePool,
    pub(crate) local_rels: FxHashMap<SyscallRef, FxHashSet<SyscallRef>>,
    pub(crate) queue: Queue,
    pub(crate) exec_handle: ExecHandle,
    pub(crate) run_history: VecDeque<Prog>,

    pub(crate) mode: Mode,
    pub(crate) mut_gaining: u32,
    pub(crate) gen_gaining: u32,
    pub(crate) cycle_len: u32,
    pub(crate) max_cycle_len: u32,

    pub(crate) work_dir: PathBuf,
    pub(crate) kernel_obj: Option<PathBuf>,
    pub(crate) kernel_src: Option<PathBuf>,

    pub(crate) last_reboot: Instant,
    pub(crate) stop: Arc<AtomicBool>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Sampling,
    Explore,
    Mutation,
}

impl Fuzzer {
    pub fn fuzz(&mut self) {
        self.sampling();

        while !self.stop() {
            if self.mode == Mode::Explore {
                self.gen();
            } else {
                self.mutate();
            }
            self.update_mode();
        }
    }

    fn sampling(&mut self) {
        let mut i = 0;
        while self.queue.current_age < 1 && i < 128 && !self.stop() {
            self.gen();
            self.mutate();
            i += 1;
        }

        if self.stop() {
            return;
        }

        let g0 = max(self.mut_gaining, 1);
        let g1 = max(self.gen_gaining, 1);
        let r0 = g0 as f64 / self.cycle_len as f64;
        let r1 = g1 as f64 / self.cycle_len as f64;

        if self.queue.current_age == 0 {
            log::warn!(
                "Fuzzer-{}: no culling occurred during the sampling, current fuzzing efficiency is too low (mutaion/gen: {}/{})",
                self.id,
                r0,
                r1
            )
        } else {
            log::info!(
                "Fuzzer-{}: sampling done, culling: {}, mutaion/gen: {}/{})",
                self.id,
                self.queue.current_age,
                r0,
                r1
            );
        }
        self.update_mode();
    }

    fn update_mode(&mut self) {
        if self.stop() || self.queue.is_empty() || self.queue.current_age == 0 {
            self.mode = Mode::Explore;
            return;
        }

        let mut rng = thread_rng();
        let g0 = max(self.mut_gaining, 1);
        let g1 = max(self.gen_gaining, 1);
        if rng.gen_ratio(g0, g0 + g1) {
            self.mode = Mode::Mutation;
        } else {
            self.mode = Mode::Explore;
        }

        let r0 = g0 as f64 / self.cycle_len as f64;
        let r1 = g1 as f64 / self.cycle_len as f64;
        if self.cycle_len < self.max_cycle_len && (r0 < 0.005 || r1 < 0.005) {
            log::info!(
                "Fuzzer-{}: gaining too low(mut/gen {}/{}), scaling circle length({} -> {})",
                self.id,
                r0,
                r1,
                self.cycle_len,
                self.cycle_len * 2
            );
            self.cycle_len *= 2;
        }
    }

    fn gen(&mut self) {
        self.gen_gaining = 0;
        for _ in 0..self.cycle_len {
            self.stats.inc_exec(EXEC_GEN);
            let p = gen(&self.target, &self.local_vals);
            if self.evaluate(p) {
                self.gen_gaining += 1;
            }
            if self.stop() {
                break;
            }
        }
    }

    fn mutate(&mut self) {
        use crate::fuzz::queue::AVG_SCORE;

        self.mut_gaining = 0;
        if self.queue.is_empty() || self.stop() {
            return;
        }

        let avg_score = self.queue.avgs[&AVG_SCORE];
        let mut mut_n = 0;

        while mut_n < self.cycle_len && !self.stop() {
            let idx = self.queue.select_idx(true);
            let n = if self.queue.inputs[idx].score > avg_score {
                4
            } else {
                2
            };

            for _ in 0..n {
                if self.stop() {
                    break;
                }
                if idx >= self.queue.len() {
                    // queue was culled, reselect.
                    break;
                }
                mut_n += 1;
                let p = mutate_args(&self.queue.inputs[idx].p);
                self.stats.inc_exec(EXEC_MUTATION);
                if self.evaluate(p) {
                    self.mut_gaining += 1;
                    self.queue.inputs[idx].update_gaining_rate(true);
                } else {
                    self.queue.inputs[idx].update_gaining_rate(false);
                }
            }

            for _ in 0..n {
                if self.stop() {
                    break;
                }
                if idx >= self.queue.len() {
                    // queue was culled, reselect.
                    break;
                }
                mut_n += 1;

                let p = seq_reuse(
                    &self.target,
                    &self.local_vals,
                    &self.queue.inputs[idx].p,
                    &self.local_rels,
                );
                self.stats.inc_exec(EXEC_MUTATION);
                if self.evaluate(p) {
                    self.mut_gaining += 1;
                    self.queue.inputs[idx].update_gaining_rate(true);
                } else {
                    self.queue.inputs[idx].update_gaining_rate(false);
                }
            }

            // TODO add more mutation methods.
        }
    }

    fn evaluate(&mut self, p: Prog) -> bool {
        if self.stop() {
            return false;
        }

        let auto_restart = Duration::new(30 * 60, 0); // 30 minutes
        let no_restart = Instant::now() - self.last_reboot;
        if no_restart > auto_restart {
            log::info!(
                "Fuzzer-{}: kernel running for {} minutes, restarting...",
                self.id,
                no_restart.as_secs() / 60,
            );
            if let Err(e) = self.exec_handle.restart() {
                log::error!("Fuzzer-{}: failed to restart: {}", self.id, e);
                exit(1);
            }
        }

        let opt = ExecOpt::new();
        let r = self.exec_handle.exec(&opt, &p);
        let exec_ret = match r {
            Ok(ret) => {
                if self.run_history.len() >= 64 {
                    self.run_history.pop_front();
                }
                self.run_history.push_back(p.clone());
                ret
            }
            Err(e) => {
                self.handle_err(e);
                return false;
            }
        };

        match exec_ret {
            ExecResult::Normal(info) => self.handle_info(p, opt, info),
            ExecResult::Failed { info, err } => self.handle_failed(p, opt, info, err, true),
            ExecResult::Crash(crash) => {
                self.handle_crash(p, opt, crash);
                true
            }
        }
    }

    fn handle_err(&self, e: ExecError) {
        match e {
            ExecError::SyzInternal(e) => {
                if e.downcast_ref::<serialize::SerializeError>().is_none() {
                    log::warn!("Fuzzer-{}: failed to execute: {}", self.id, e);
                }
            }
            ExecError::Spawn(e) => {
                log::warn!("Fuzzer-{}: failed to execute: {}", self.id, e);
                // TODO
            }
        }
    }

    fn handle_failed(
        &mut self,
        mut p: Prog,
        opt: ExecOpt,
        mut info: Vec<CallExecInfo>,
        e: Box<dyn std::error::Error + 'static>,
        hanlde_info: bool,
    ) -> bool {
        log::info!("Fuzzer-{}: prog failed: {}", self.id, e);

        if !hanlde_info || self.stop() {
            return false;
        }

        let mut has_brs = false;
        for i in (0..info.len()).rev() {
            if info[i].branches.is_empty() {
                info.remove(i);
                p.calls.drain(i..p.calls.len());
            } else {
                has_brs = true;
                break;
            }
        }

        if has_brs {
            self.handle_info(p, opt, info)
        } else {
            false
        }
    }

    fn handle_info(&mut self, p: Prog, opt: ExecOpt, info: Vec<CallExecInfo>) -> bool {
        if self.stop() {
            return false;
        }

        let (has_new, mut new_brs) = self.check_max_cov(&info);
        if !has_new {
            return false;
        }

        let (succ, _) = self.calibrate_cov(&p, &mut new_brs);
        if !succ {
            return false;
        }

        let mut analyzed: FxHashSet<usize> = FxHashSet::default();
        let mut new_input = false;
        // analyze in reverse order helps us find interesting longger prog.
        for i in (0..info.len()).rev() {
            if info[i].branches.is_empty() || analyzed.contains(&i) || self.stop() {
                continue;
            }

            let mini_ret = self.minimize(p.sub_prog(i), &info[0..i + 1], &new_brs[i]);
            if mini_ret.is_none() {
                continue;
            }
            let (m_p, calls_idx, m_p_info) = mini_ret.unwrap(); // minimized prog.
            analyzed.extend(&calls_idx);
            let mut m_p_brs = calls_idx
                .into_iter()
                .map(|idx| new_brs[idx].iter().copied().collect())
                .collect::<Vec<_>>();

            let (succ, exec_tm) = self.calibrate_cov(&m_p, &mut m_p_brs);
            if !succ {
                continue;
            }

            new_input = true;
            let new_re = self.detect_relations(&m_p, &m_p_brs);

            let mut input = Input::new(m_p, opt.clone(), m_p_info);
            input.found_new_re = new_re;
            input.exec_tm = exec_tm.as_millis() as usize;
            input.new_cov = m_p_brs.into_iter().flatten().collect();
            self.queue.append(input);
        }
        new_input && !self.stop()
    }

    fn minimize(
        &mut self,
        mut p: Prog,
        old_call_infos: &[CallExecInfo],
        new_brs: &FxHashSet<u32>,
    ) -> Option<(Prog, Vec<usize>, Vec<CallExecInfo>)> {
        if p.calls.len() <= 1 || self.stop() {
            return None;
        }

        let mut call_infos = None;
        let opt = ExecOpt::new();
        let old_len = p.calls.len();
        let mut removed = Vec::new();
        let mut idx = p.calls.len() - 1;

        for i in (0..p.calls.len() - 1).rev() {
            if self.stop() {
                return None;
            }

            let new_p = p.remove(i);
            idx -= 1;
            self.stats.inc_exec(EXEC_MINIMIZE);
            let ret = self.exec_handle.exec(&opt, &new_p);
            if let Some(info) = self.handle_ret_comm(&new_p, opt.clone(), ret) {
                let brs = info[idx].branches.iter().copied().collect::<FxHashSet<_>>();
                if brs.is_superset(new_brs) {
                    p = new_p;
                    removed.push(i);
                    call_infos = Some(info);
                } else {
                    idx += 1;
                }
            } else {
                return None;
            }
        }

        let reserved = (0..old_len)
            .filter(|i| !removed.contains(i))
            .collect::<Vec<_>>();
        if reserved.len() > 2 {
            log::info!(
                "Fuzzer-{}: minimized success: {} -> {}",
                self.id,
                old_len,
                reserved.len()
            );
        }

        Some((
            p,
            reserved,
            call_infos.unwrap_or_else(|| Vec::from(old_call_infos)),
        ))
    }

    fn detect_relations(&mut self, p: &Prog, brs: &[FxHashSet<u32>]) -> bool {
        if p.calls.len() == 1 || self.stop() {
            return false;
        }

        let mut detected = false;
        let opt = ExecOpt::new_no_collide();
        for i in 0..p.calls.len() - 1 {
            if self.stop() {
                return false;
            }

            if self.local_rels.contains_key(p.calls[i].meta) {
                if self.local_rels[p.calls[i].meta].contains(p.calls[i + 1].meta) {
                    continue;
                }
            }

            let new_p = p.remove(i);
            self.stats.inc_exec(EXEC_RDETECT);
            let ret = self.exec_handle.exec(&opt, &new_p);
            if let Some(info) = self.handle_ret_comm(&new_p, opt.clone(), ret) {
                if !brs[i].iter().all(|br| info[i].branches.contains(br)) {
                    let s0 = p.calls[i].meta;
                    let s1 = new_p.calls[i].meta;
                    if self.add_relation((s0, s1)) {
                        detected = true
                    }
                }
            }
        }

        detected
    }

    fn add_relation(&mut self, (s0, s1): (SyscallRef, SyscallRef)) -> bool {
        let new;
        {
            let r = self.relations.read().unwrap();
            if !r.contains_key(&s0) {
                new = true;
            } else {
                new = !r[&s0].contains(&s1);
            }
            for (key, v) in r.iter() {
                let entry = self.local_rels.entry(key).or_default();
                entry.extend(v.iter().copied());
            }
        }
        let entry = self.local_rels.entry(s0).or_default();
        entry.insert(s1);
        if new {
            log::info!(
                "Fuzzer-{}: detect new relation: ({}, {})",
                self.id,
                s0.name,
                s1.name
            );
            let mut r = self.relations.write().unwrap();
            let entry = r.entry(s0).or_default();
            entry.insert(s1);
        }
        new
    }

    fn check_max_cov(&self, info: &[CallExecInfo]) -> (bool, Vec<FxHashSet<u32>>) {
        let mut has_new = false;
        let mut new_brs = Vec::with_capacity(info.len());

        {
            let covs = self.max_cov.read().unwrap();
            for i in info.iter() {
                let mut new_br = FxHashSet::default();
                for br in i.branches.iter().copied() {
                    if !covs.contains(&br) {
                        new_br.insert(br);
                    }
                }
                if !new_br.is_empty() {
                    has_new = true;
                }
                new_brs.push(new_br);
            }
        }
        if has_new {
            let mut covs = self.max_cov.write().unwrap();
            for br in new_brs.iter() {
                covs.extend(br.iter().copied());
            }
            self.stats.store(OVERALL_MAX_COV, covs.len() as u64);
        }
        (has_new, new_brs)
    }

    fn calibrate_cov(&mut self, p: &Prog, new_covs: &mut [FxHashSet<u32>]) -> (bool, Duration) {
        let opt = ExecOpt::new_no_collide();
        let mut failed = false;
        let mut exec_tm = Duration::new(0, 0);

        for _ in 0..3 {
            if self.stop() {
                return (false, Duration::default());
            }

            let now = Instant::now();
            let ret = self.exec_handle.exec(&opt, p);
            exec_tm += now.elapsed();
            self.stats.inc_exec(EXEC_CALIBRATE);
            if let Some(info) = self.handle_ret_comm(p, opt.clone(), ret) {
                for (i, call_info) in info.into_iter().enumerate() {
                    let brs = FxHashSet::from_iter(call_info.branches.into_iter());
                    new_covs[i] = new_covs[i].intersection(&brs).copied().collect();
                }
            } else {
                failed = true;
                break;
            }
        }

        let succ = !failed && new_covs.iter().any(|c| !c.is_empty()) && !self.stop();
        if succ {
            let mut covs = self.calibrated_cov.write().unwrap();
            for br in new_covs.iter() {
                covs.extend(br.iter().copied());
            }
            self.stats.store(OVERALL_CAL_COV, covs.len() as u64);
        }

        (succ, exec_tm / 3)
    }

    fn handle_ret_comm(
        &mut self,
        p: &Prog,
        opt: ExecOpt,
        ret: Result<ExecResult, ExecError>,
    ) -> Option<Vec<CallExecInfo>> {
        match ret {
            Ok(exec_ret) => match exec_ret {
                ExecResult::Normal(info) => Some(info),
                ExecResult::Failed { info, err } => {
                    self.handle_failed(p.clone(), opt, info, err, false);
                    None
                }
                ExecResult::Crash(c) => {
                    self.handle_crash(p.clone(), opt, c);
                    None
                }
            },
            Err(e) => {
                self.handle_err(e);
                None
            }
        }
    }

    fn handle_crash(&mut self, p: Prog, _opt: ExecOpt, info: CrashInfo) {
        if self.stop() {
            return; // killed by ctrlc
        }
        self.last_reboot = Instant::now();

        log::info!(
            "Fuzzer-{}: kernel crashed, maybe caused by:\n {}",
            self.id,
            p.to_string()
        );
        log::info!("Fuzzer-{}: trying to extract reports ...", self.id);
        self.stats.update_time(OVERALL_LAST_CRASH);
        self.stats.inc(OVERALL_TOTAL_CRASHES);

        let log = info.qemu_stdout;
        let log_file = temp_dir().join(format!("healer-crash-log-{}.tmp", self.id));

        if let Err(e) = write(&log_file, &log) {
            log::error!(
                "Fuzzer-{}: failed to write crash log to tmp file '{}': {}",
                self.id,
                log_file.display(),
                e
            );
            self.save_raw_log(p, log);
            return;
        }

        let bin_path = self.work_dir.join("bin").join("syz-symbolize");
        let mut syz_symbolize = Command::new(&bin_path);
        syz_symbolize
            .args(vec!["-os", &self.target.os])
            .args(vec!["-arch", &self.target.arch]);
        if let Some(kernel_obj) = self.kernel_obj.as_ref() {
            syz_symbolize.arg("-kernel_obj").arg(kernel_obj);
        }
        if let Some(kernel_src) = self.kernel_src.as_ref() {
            syz_symbolize.arg("-kernel_src").arg(kernel_src);
        }
        syz_symbolize.arg(&log_file);
        let output = syz_symbolize.output().unwrap();

        if output.status.success() {
            let mut reports = parse(&output.stdout);
            if !reports.is_empty() {
                let mut titles = String::new();
                for (i, mut r) in reports.iter_mut().enumerate() {
                    r.prog = Some(ProgWrapper(p.clone()));
                    r.raw_log = log.clone();
                    write!(titles, "\n\t{}. {}", i + 1, r.title).unwrap();
                }
                log::info!("Fuzzer-{}: report extracted: {}", self.id, titles);
                self.save_reports(reports);
                return;
            }
        }

        let err = String::from_utf8_lossy(&output.stderr);
        log::warn!(
            "Fuzzer-{}: failed to extract crash report: {}",
            self.id,
            err
        );
        self.save_raw_log(p, log);
    }

    fn save_raw_log(&mut self, p: Prog, log: Vec<u8>) {
        let mut next_id;
        let p_str = p.to_string();

        {
            let mut raw_crashes = self.raw_crashes.lock().unwrap();
            next_id = raw_crashes.len();
            if raw_crashes.len() >= 1024 {
                raw_crashes.pop_front();
                next_id = raw_crashes.back().unwrap().id + 1;
            }
            let r = Report {
                id: next_id,
                prog: Some(ProgWrapper(p)),
                ..Default::default()
            };
            raw_crashes.push_back(r);
        }

        let out_dir = self
            .work_dir
            .join("crashes")
            .join("raw_logs")
            .join(next_id.to_string());
        if let Err(e) = create_dir_all(&out_dir) {
            if e.kind() != ErrorKind::AlreadyExists {
                log::error!(
                    "Fuzzer-{}: failed to create dir '{}': {}",
                    self.id,
                    out_dir.display(),
                    e
                );
                exit(1);
            }
        }
        write(out_dir.join("log"), &log).unwrap();
        write(out_dir.join("prog"), p_str).unwrap();
    }

    fn save_reports(&mut self, reports: Vec<Report>) {
        for r in reports {
            let mut id = 0;
            let r1 = r.clone();
            {
                let mut crashes = self.crashes.lock().unwrap();
                if let Some(reports) = crashes.get_mut(&r.title) {
                    id = reports.len();
                    if reports.len() >= 1024 {
                        reports.pop_front();
                        id = reports.back().unwrap().id + 1;
                    }
                    reports.push_back(r);
                } else {
                    let t = r.title.clone();
                    let mut reports = VecDeque::with_capacity(1024);
                    reports.push_back(r);
                    crashes.insert(t, reports);
                }
            }

            let mut out_dir_name = r1.title.clone();
            if out_dir_name.len() >= 255 {
                let mut hasher = FxHasher::default();
                out_dir_name.hash(&mut hasher);
                let hash = hasher.finish();
                out_dir_name = format!("{:X}", hash);
            }
            let out_dir = self.work_dir.join("crashes").join(&out_dir_name);

            if id == 0 {
                self.stats.inc(OVERALL_UNIQUE_CRASHES);
                if let Err(e) = create_dir_all(&out_dir) {
                    if e.kind() != ErrorKind::AlreadyExists {
                        log::error!(
                            "Fuzzer-{}: failed to create dir '{}': {}",
                            self.id,
                            out_dir.display(),
                            e
                        );
                        exit(1);
                    }
                }
                let mut meta = String::new();
                writeln!(meta, "TITLE: {}", r1.title).unwrap();
                if let Some(cor) = r1.corrupted.as_ref() {
                    writeln!(meta, "CORRUPTED: {}", cor).unwrap();
                }
                if !r1.to_mails.is_empty() {
                    writeln!(meta, "MAINTAINERS (TO): {:?}", r1.to_mails).unwrap();
                }
                if !r1.cc_mails.is_empty() {
                    writeln!(meta, "MAINTAINERS (TO): {:?}", r1.cc_mails).unwrap();
                }
                write(out_dir.join("meta"), meta).unwrap();
            }
            write(
                out_dir.join(format!("prog{}", id)),
                r1.prog.as_ref().unwrap().0.to_string(),
            )
            .unwrap();
            write(out_dir.join(format!("report{}", id)), r1.report).unwrap();
            write(out_dir.join(format!("log{}", id)), r1.raw_log).unwrap();
        }
    }

    #[inline(always)]
    fn stop(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

#[derive(Default, Clone)]
pub struct Report {
    pub(crate) id: usize,
    pub(crate) title: String,
    pub(crate) corrupted: Option<String>,
    pub(crate) to_mails: Vec<String>,
    pub(crate) cc_mails: Vec<String>,
    pub(crate) report: String,
    pub(crate) raw_log: Vec<u8>,
    pub(crate) prog: Option<ProgWrapper>,
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
