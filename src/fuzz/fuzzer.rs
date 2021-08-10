use crate::{
    exec::syz::{
        CallExecInfo, ExecOpt, SyzExecError, SyzExecHandle, SyzExecResult, CALL_FAULT_INJECTED,
        FLAG_INJECT_FAULT,
    },
    fuzz::{
        features::*,
        input::Input,
        mutation::MUTATE_OP,
        queue::Queue,
        relation::Relation,
        repro::{Repro, ReproResult},
        stats::*,
    },
    gen::gen,
    model::{Prog, ProgWrapper},
    targets::Target,
    utils::stop_soon,
    vm::{qemu::QemuHandle, ManageVm},
    Config,
};

use std::{
    cmp::max,
    collections::VecDeque,
    env::temp_dir,
    fmt::Write,
    fs::{create_dir_all, write},
    hash::{Hash, Hasher},
    io::ErrorKind,
    process::{exit, Command},
    sync::RwLock,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

pub struct Fuzzer {
    // shared between different fuzzers.
    // TODO sync prog between diffierent threads.
    // progs: Arc<RwLock<FxHashMap<usize, Vec<ProgWrapper>>>>,
    pub(crate) max_cov: Arc<RwLock<FxHashSet<u32>>>,
    pub(crate) calibrated_cov: Arc<RwLock<FxHashSet<u32>>>,
    pub(crate) relations: Arc<Relation>,
    pub(crate) crashes: Arc<Mutex<FxHashMap<String, VecDeque<Report>>>>,
    pub(crate) white_list: Arc<FxHashSet<String>>,
    pub(crate) repros: Arc<Mutex<FxHashMap<String, Repro>>>,
    pub(crate) reproducing: Arc<Mutex<FxHashSet<String>>>,
    pub(crate) raw_crashes: Arc<Mutex<VecDeque<Report>>>,
    pub(crate) stats: Arc<Stats>,

    // local data.
    pub(crate) id: u64,
    pub(crate) conf: Config,
    pub(crate) features: u64,
    pub(crate) target: Target,
    pub(crate) queue: Queue,
    pub(crate) exec_handle: SyzExecHandle<<QemuHandle as ManageVm>::Error>,
    pub(crate) run_history: VecDeque<(ExecOpt, Prog)>,

    pub(crate) mut_gaining: u32,
    pub(crate) gen_gaining: u32,
    pub(crate) cycle_len: u32,

    pub(crate) last_reboot: Instant,
}

impl Fuzzer {
    pub fn fuzz(&mut self) {
        self.stats.inc(OVERALL_FUZZ_INSTANCE);
        self.sampling();

        let mut i: u64 = 0;
        while !stop_soon() {
            if i % 60 == 0 {
                self.gen();
            } else {
                self.mutate();
            }
            i += 1;
        }
    }

    fn sampling(&mut self) {
        let mut i = 0;
        while self.queue.current_age < 1 && i < 128 && !stop_soon() {
            self.gen();
            self.mutate();
            i += 1;
        }

        if stop_soon() {
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
        //  self.update_mode();
    }

    // fn update_mode(&mut self) {
    //     if stop_soon() || self.queue.is_empty() || self.queue.current_age == 0 {
    //         self.mode = Mode::Explore;
    //         return;
    //     }

    //     let mut rng = thread_rng();
    //     let g0 = max(self.mut_gaining, 1);
    //     let g1 = max(self.gen_gaining, 1);
    //     if rng.gen_ratio(g0, g0 + g1) {
    //         self.mode = Mode::Mutation;
    //     } else {
    //         self.mode = Mode::Explore;
    //     }
    // }

    fn gen(&mut self) {
        self.gen_gaining = 0;
        for _ in 0..self.cycle_len {
            self.stats.inc_exec(EXEC_GEN);
            let p = gen(&self.target);
            if self.evaluate(p) {
                self.gen_gaining += 1;
            }
            if stop_soon() {
                break;
            }
        }
    }

    fn mutate(&mut self) {
        use crate::fuzz::queue::AVG_SCORE;

        self.mut_gaining = 0;
        if self.queue.is_empty() || stop_soon() {
            return;
        }

        let avg_score = self.queue.avgs[&AVG_SCORE];
        let mut mut_n = 0;

        while mut_n < self.cycle_len && !stop_soon() {
            let idx = self.queue.select_idx(true);
            let n = if self.queue.inputs[idx].score > avg_score {
                4
            } else {
                2
            };

            for mutation_op in MUTATE_OP.iter().copied() {
                for _ in 0..n {
                    if stop_soon() {
                        break;
                    }
                    if idx >= self.queue.len() {
                        // queue was culled, reselect.
                        break;
                    }
                    mut_n += 1;
                    let p = mutation_op(&self.target, &self.queue.inputs[idx].p);
                    self.stats.inc_exec(EXEC_MUTATION);
                    if self.evaluate(p) {
                        self.mut_gaining += 1;
                    }
                }
            }

            // for _ in 0..n {
            //     if stop_soon() {
            //         break;
            //     }
            //     if idx >= self.queue.len() {
            //         // queue was culled, reselect.
            //         break;
            //     }
            //     mut_n += 1;

            //     let p = seq_reuse(
            //         &self.target,
            //         &self.queue.inputs[idx].p,
            //         &self.relations,
            //     );
            //     self.stats.inc_exec(EXEC_MUTATION);
            //     if self.evaluate(p) {
            //         self.mut_gaining += 1;
            //     }
            // }

            if self.features & FEATURE_FAULT != 0
                && idx < self.queue.len()
                && !self.queue.inputs[idx].fault_injected
            {
                let p = self.queue.inputs[idx].p.clone();
                self.fail_call(p);
                self.queue.inputs[idx].fault_injected = true;
            }

            // TODO add more mutation methods.
        }
    }

    fn fail_call(&mut self, p: Prog) {
        for i in 0..p.calls.len() {
            for n in 0..100 {
                let mut opt = ExecOpt::new();
                opt.flags |= FLAG_INJECT_FAULT;
                opt.fault_call = i as i32;
                opt.fault_nth = n;
                let ret = self.exec(&opt, &p);
                if let Some(info) = self.handle_ret_comm(&p, opt, ret) {
                    if info.len() > i && info[i].flags & CALL_FAULT_INJECTED == 0 {
                        break;
                    }
                }
            }
        }
    }

    fn evaluate(&mut self, p: Prog) -> bool {
        if stop_soon() {
            return false;
        }

        let auto_restart = Duration::new(60 * 60, 0); // 60 minutes
        let no_restart = Instant::now() - self.last_reboot;
        if no_restart > auto_restart && !stop_soon() {
            log::info!(
                "Fuzzer-{}: kernel running for {} minutes, restarting...",
                self.id,
                no_restart.as_secs() / 60,
            );
            if let Err(e) = self.exec_handle.spawn_syz(true) {
                log::error!("Fuzzer-{}: failed to restart: {}", self.id, e);
                exit(1);
            }
            self.last_reboot = Instant::now();
        }

        let opt = ExecOpt::new();
        let r = self.exec(&opt, &p);
        let exec_ret = match r {
            Ok(ret) => ret,
            Err(e) => {
                self.handle_err(e);
                return false;
            }
        };

        match exec_ret {
            SyzExecResult::Normal(info) => self.handle_info(p, opt, info),
            SyzExecResult::Crash(crash) => {
                self.handle_crash(p, opt, crash);
                true
            }
        }
    }

    fn exec(&mut self, opt: &ExecOpt, p: &Prog) -> Result<SyzExecResult, SyzExecError> {
        let ret = self.exec_handle.exec(&self.target, p, opt);
        if ret.is_ok() {
            if self.run_history.len() >= 256 {
                self.run_history.pop_front();
            }
            self.run_history.push_back((opt.clone(), p.clone()));
        }
        ret
    }

    fn handle_err(&self, e: SyzExecError) {
        match e {
            SyzExecError::Serialize(_) | SyzExecError::Parse(_) => {}
            _ => panic!("{}", e),
        }
    }

    // fn handle_failed(
    //     &mut self,
    //     mut p: Prog,
    //     opt: ExecOpt,
    //     mut info: Vec<CallExecInfo>,
    //     e: Box<dyn std::error::Error + 'static>,
    //     hanlde_info: bool,
    // ) -> bool {
    //     log::info!("Fuzzer-{}: prog failed: {}", self.id, e);

    //     if !hanlde_info || stop_soon() {
    //         return false;
    //     }

    //     let mut has_brs = false;
    //     for i in (0..info.len()).rev() {
    //         if info[i].branches.is_empty() {
    //             info.remove(i);
    //             p.calls.drain(i..p.calls.len());
    //         } else {
    //             has_brs = true;
    //             break;
    //         }
    //     }

    //     if has_brs {
    //         self.handle_info(p, opt, info)
    //     } else {
    //         false
    //     }
    // }

    fn handle_info(&mut self, p: Prog, opt: ExecOpt, info: Vec<CallExecInfo>) -> bool {
        if stop_soon() {
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
        // analyze in reverse order helps us find longger prog.
        for i in (0..info.len()).rev() {
            if info[i].branches.is_empty() || analyzed.contains(&i) || stop_soon() {
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
            let mut new_re = false;
            if !self.conf.disable_relation_detect {
                new_re = self.detect_relations(&m_p, &m_p_brs);
            }

            let mut input = Input::new(
                m_p,
                opt.clone(),
                m_p_info,
                m_p_brs.into_iter().flatten().collect(),
            );
            input.found_new_re = new_re;
            input.exec_tm = exec_tm.as_millis() as usize;
            self.queue.append(input);
        }

        new_input
    }

    fn minimize(
        &mut self,
        mut p: Prog,
        old_call_infos: &[CallExecInfo],
        new_brs: &FxHashSet<u32>,
    ) -> Option<(Prog, Vec<usize>, Vec<CallExecInfo>)> {
        if p.calls.len() <= 1 || stop_soon() {
            return None;
        }

        let mut call_infos = None;
        let opt = ExecOpt::new();
        let old_len = p.calls.len();
        let mut removed = Vec::new();
        let mut idx = p.calls.len() - 1;

        for i in (0..p.calls.len() - 1).rev() {
            if stop_soon() {
                return None;
            }

            let new_p = p.remove(i);
            idx -= 1;
            self.stats.inc_exec(EXEC_MINIMIZE);
            let ret = self.exec(&opt, &new_p);
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
        if p.calls.len() == 1 || stop_soon() {
            return false;
        }

        let mut detected = false;
        let opt = ExecOpt::new_no_collide();
        for i in 0..p.calls.len() - 1 {
            if stop_soon() {
                return false;
            }

            if self
                .relations
                .contains(p.calls[i].meta, p.calls[i + 1].meta)
            {
                continue;
            }

            let new_p = p.remove(i);
            self.stats.inc_exec(EXEC_RDETECT);
            let ret = self.exec_handle.exec(&self.target, &new_p, &opt);
            if let Some(info) = self.handle_ret_comm(&new_p, opt.clone(), ret) {
                if !brs[i].iter().all(|br| info[i].branches.contains(br)) {
                    let s0 = p.calls[i].meta;
                    let s1 = new_p.calls[i].meta;
                    match self.relations.insert(s0, s1) {
                        Ok(new) => {
                            if new {
                                detected = new;
                                log::info!(
                                    "Fuzzer-{}: detect new relation: ({}, {})",
                                    self.id,
                                    s0.name,
                                    s1.name
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!(
                                "Fuzzer-{}: failed to persist relation ({}, {}): {}",
                                self.id,
                                s0,
                                s1,
                                e
                            );
                        }
                    }
                }
            }
        }

        detected
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

        for _ in 0..1 {
            if stop_soon() {
                return (false, Duration::default());
            }

            let now = Instant::now();
            let ret = self.exec_handle.exec(&self.target, p, &opt);
            exec_tm += now.elapsed();
            self.stats.inc_exec(EXEC_CALIBRATE);
            if let Some(info) = self.handle_ret_comm(p, opt.clone(), ret) {
                for (i, call_info) in info.into_iter().enumerate() {
                    let brs = call_info.branches.into_iter().collect();
                    new_covs[i] = new_covs[i].intersection(&brs).copied().collect();
                }
            } else {
                failed = true;
                break;
            }
        }

        let succ = !failed && new_covs.iter().any(|c| !c.is_empty());
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
        ret: Result<SyzExecResult, SyzExecError>,
    ) -> Option<Vec<CallExecInfo>> {
        match ret {
            Ok(exec_ret) => match exec_ret {
                SyzExecResult::Normal(info) => {
                    if info.is_empty() {
                        None
                    } else {
                        Some(info)
                    }
                }
                SyzExecResult::Crash(c) => {
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

    fn handle_crash(&mut self, p: Prog, _opt: ExecOpt, log: Vec<u8>) {
        if stop_soon() {
            return; // killed by ctrlc
        }

        self.last_reboot = Instant::now();
        self.stats.update_time(OVERALL_LAST_CRASH);
        self.stats.inc(OVERALL_TOTAL_CRASHES);

        log::info!(
            "Fuzzer-{}: kernel crashed, maybe caused by:\n {}",
            self.id,
            p.to_string()
        );
        log::info!("Fuzzer-{}: try to extract reports ...", self.id);

        let reports = self.extract_report(&p, &log);
        if !reports.is_empty() {
            let title = reports[0].title.clone();
            log::info!("Fuzzer-{}: report extracted: {}", self.id, title);
            self.save_reports(&title, reports);
            self.try_repro(&title, log);
        } else {
            log::info!(
                "Fuzzer-{}: failed to extract reports, save log and prog only",
                self.id
            );
            self.save_raw_log(p, log);
        }
    }

    fn extract_report(&self, p: &Prog, raw_log: &[u8]) -> Vec<Report> {
        let log_file = temp_dir().join(format!("healer-crash-log-{}.tmp", self.id));
        let mut ret = Vec::new();
        if let Err(e) = write(&log_file, raw_log) {
            log::error!(
                "Fuzzer-{}: failed to write crash log to tmp file '{}': {}",
                self.id,
                log_file.display(),
                e
            );
            return ret;
        }

        let mut syz_symbolize = Command::new(&self.conf.syz_bin_dir.join("syz-symbolize"));
        syz_symbolize
            .args(vec!["-os", &self.target.os])
            .args(vec!["-arch", &self.target.arch]);
        if let Some(kernel_obj) = self.conf.kernel_obj_dir.as_ref() {
            syz_symbolize.arg("-kernel_obj").arg(kernel_obj);
        }
        if let Some(kernel_src) = self.conf.kernel_src_dir.as_ref() {
            syz_symbolize.arg("-kernel_src").arg(kernel_src);
        }
        syz_symbolize.arg(&log_file);
        let output = syz_symbolize.output().unwrap();

        if output.status.success() {
            ret = parse(&output.stdout);
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            log::warn!("Fuzzer-{}: syz-symbolize: {}", self.id, err);
        }

        for r in ret.iter_mut() {
            r.prog = Some(ProgWrapper(p.clone()));
            r.raw_log = Vec::from(raw_log);
        }

        ret
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
            .conf
            .out_dir
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

    fn save_reports(&mut self, title: &str, mut reports: Vec<Report>) {
        let mut id = 0;
        let n = reports.len();

        {
            let mut crashes = self.crashes.lock().unwrap();
            let reports_all = crashes
                .entry(title.to_string())
                .or_insert_with(|| VecDeque::with_capacity(1024));
            if !reports_all.is_empty() {
                id = reports_all.back().unwrap().id + 1;
                if reports_all.len() >= 1024 {
                    for _ in 0..reports.len() {
                        reports_all.pop_front();
                    }
                }
            }
            for r in reports.iter_mut() {
                r.id = id;
                reports_all.push_back(r.clone());
            }
        }

        let dir_name = Self::dir_name(title);
        let out_dir = self.conf.out_dir.join("crashes").join(&dir_name);
        for (sub_id, r) in reports.into_iter().enumerate() {
            if id == 0 && sub_id == 0 {
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
                writeln!(meta, "TITLE: {}", title).unwrap();
                if let Some(cor) = r.corrupted.as_ref() {
                    writeln!(meta, "CORRUPTED: {}", cor).unwrap();
                }
                if !r.to_mails.is_empty() {
                    writeln!(meta, "MAINTAINERS (TO): {:?}", r.to_mails).unwrap();
                }
                if !r.cc_mails.is_empty() {
                    writeln!(meta, "MAINTAINERS (CC): {:?}", r.cc_mails).unwrap();
                }
                write(out_dir.join("meta"), meta).unwrap();
            }

            if sub_id == 0 {
                write(
                    out_dir.join(format!("prog{}", id)),
                    r.prog.as_ref().unwrap().0.to_string(),
                )
                .unwrap();
                write(out_dir.join(format!("log{}", id)), r.raw_log).unwrap();
            }

            if n != 1 {
                write(out_dir.join(format!("report{}-{}", id, sub_id)), r.report).unwrap();
            } else {
                write(out_dir.join(format!("report{}", id)), r.report).unwrap();
            }
        }
    }

    fn dir_name(title: &str) -> String {
        let mut title = title.replace('/', "~");
        if title.len() >= 255 {
            let mut hasher = FxHasher::default();
            title.hash(&mut hasher);
            let hash = hasher.finish();
            title = format!("{:X}", hash);
        }
        title
    }

    fn try_repro(&mut self, title: &str, crash_log: Vec<u8>) {
        if self.conf.skip_repro || !self.need_repro(title) || stop_soon() {
            self.run_history.clear();
            return;
        }

        self.stats.dec(OVERALL_FUZZ_INSTANCE);
        self.stats.inc(OVERALL_REPRO_INSTANCE);

        {
            let mut reproducing = self.reproducing.lock().unwrap();
            reproducing.insert(title.to_string());
        }

        log::info!("Fuzzer-{}: try to repro '{}'", self.id, title);
        let repro_start = Instant::now();
        let res = self.repro(&crash_log);

        if let Err(e) = res {
            log::info!("Fuzzer-{}: failed to repro '{}': {}", self.id, title, e);
        } else {
            let res = res.unwrap();
            match res {
                ReproResult::Succ(repro) => {
                    log::info!(
                        "Fuzzer-{}: repro '{}' success ({}s), crepro: {}",
                        self.id,
                        title,
                        repro_start.elapsed().as_secs(),
                        repro.c_prog.is_some()
                    );
                    self.save_repro(title, repro);
                }
                ReproResult::Failed(msg) => {
                    log::info!("Fuzzer-{}: failed to repro '{}': {}", self.id, title, msg);
                }
            }
        }

        self.stats.inc(OVERALL_FUZZ_INSTANCE);
        self.stats.dec(OVERALL_REPRO_INSTANCE);
        {
            let mut reproducing = self.reproducing.lock().unwrap();
            reproducing.remove(title);
        }
        self.run_history.clear();
    }

    fn need_repro(&self, title: &str) -> bool {
        // TODO add more filter rule.
        if self.white_list.contains(title) {
            return false;
        }

        {
            let reproducing = self.reproducing.lock().unwrap();
            if reproducing.contains(title) {
                return false;
            }
        }
        let repros = self.repros.lock().unwrap();
        !repros.contains_key(title)
    }

    fn save_repro(&mut self, title: &str, repro: Repro) {
        let out_dir = self
            .conf
            .out_dir
            .join("crashes")
            .join(Self::dir_name(title));

        let mut prog = format!("# {}\n\n", repro.opt);
        prog.push_str(&repro.p);
        let fname = out_dir.join("repro.prog");
        write(&fname, prog.as_bytes()).unwrap_or_else(|e| {
            log::error!(
                "Fuzzer-{}: failed to write repro prog to {}: {}",
                self.id,
                fname.display(),
                e
            );
            exit(1)
        });

        let fname = out_dir.join("run_history");
        write(&fname, repro.log.as_bytes()).unwrap_or_else(|e| {
            log::error!(
                "Fuzzer-{}: failed to write run history to {}: {}",
                self.id,
                fname.display(),
                e
            );
            exit(1)
        });

        let fname = out_dir.join("repro.log");
        write(&fname, repro.repro_log.as_bytes()).unwrap_or_else(|e| {
            log::error!(
                "Fuzzer-{}: failed to write repro log to {}: {}",
                self.id,
                fname.display(),
                e
            );
            exit(1)
        });

        if let Some(cprog) = repro.c_prog.as_ref() {
            let fname = out_dir.join("repro.cprog");
            write(&fname, cprog.as_bytes()).unwrap_or_else(|e| {
                log::error!(
                    "Fuzzer-{}: failed to write repro cprog to {}: {}",
                    self.id,
                    fname.display(),
                    e
                );
                exit(1)
            });
        }

        let mut repros = self.repros.lock().unwrap();
        repros.insert(title.to_string(), repro);
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
