use crate::{
    fuzz::{input::Input, stats},
    model::SyscallRef,
};

use std::{
    fmt::Write,
    fs::{create_dir_all, write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use iota::iota;
use rand::{prelude::*, random, thread_rng, Rng};
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;

iota! {
    pub const AVG_SZ: usize = iota;
        , AVG_AGE
        , AVG_EXEC_TM
        , AVG_RES_CNT
        , AVG_NEW_COV
        , AVG_LEN
        , AVG_SCORE
}

pub struct Queue {
    pub(crate) id: usize,
    pub(crate) inputs: Vec<Input>,

    pub(crate) last_num: usize,
    pub(crate) last_culling: Instant,
    pub(crate) culling_threshold: usize,
    pub(crate) culling_duration: Duration,

    pub(crate) favored: Vec<usize>,
    pub(crate) pending_favored: Vec<usize>,
    pub(crate) found_re: Vec<usize>,
    pub(crate) pending_found_re: Vec<usize>,
    pub(crate) scores: Vec<usize>,
    pub(crate) score_sum: usize,
    pub(crate) input_depth: Vec<Vec<usize>>,

    pub(crate) current_age: usize,
    pub(crate) avgs: FxHashMap<usize, usize>,
    pub(crate) call_cnt: FxHashMap<SyscallRef, usize>,
    pub(crate) stats: Option<Arc<stats::Stats>>,
    pub(crate) out_dir: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Unimplemented(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl Queue {
    pub fn with_outdir(id: usize, out_dir: PathBuf) -> Result<Self, Error> {
        let queue_dir = out_dir.join(format!("queue-{}", id));
        if queue_dir.is_dir() {
            Self::load(id, queue_dir)
        } else {
            Ok(Self::new(id, Some(queue_dir)))
        }
    }

    pub fn new(id: usize, queue_dir: Option<PathBuf>) -> Self {
        let avgs = hashmap! {
            AVG_SZ => 0,
            AVG_AGE => 0,
            AVG_EXEC_TM => 0,
            AVG_RES_CNT => 0,
            AVG_NEW_COV => 0,
            AVG_LEN => 0,
            AVG_SCORE => 0
        };

        Self {
            id,
            inputs: Vec::new(),

            last_num: 0,
            last_culling: Instant::now(),
            culling_threshold: 128,
            culling_duration: Duration::from_secs(15 * 60),

            favored: Vec::new(),
            pending_favored: Vec::new(),
            found_re: Vec::new(),
            pending_found_re: Vec::new(),
            scores: Vec::new(),
            score_sum: 0,
            input_depth: Vec::new(),

            current_age: 0,
            avgs,
            call_cnt: FxHashMap::default(),
            stats: None,
            out_dir: queue_dir,
        }
    }

    pub fn load<P: AsRef<Path>>(_id: usize, f: P) -> Result<Self, Error> {
        Err(Error::Unimplemented(format!(
            "in-place resume not implemented for the queue, please remove old data '{}' first",
            f.as_ref().display()
        )))
    }

    pub fn set_stats(&mut self, stats: Arc<stats::Stats>) {
        self.stats = Some(stats)
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    pub fn select(&mut self, to_mutate: bool) -> &mut Input {
        let idx = self.select_idx(to_mutate);
        &mut self.inputs[idx]
    }

    pub fn select_idx(&mut self, to_mutate: bool) -> usize {
        assert!(
            !self.inputs.is_empty(),
            "try to select from an empty input queue"
        );

        let mut rng = thread_rng();
        // select pending
        if !self.pending_favored.is_empty() {
            let idx = Self::select_pending(&mut self.pending_favored, &mut self.inputs, to_mutate);
            self.update_queue_stats();
            idx
        } else if !self.pending_found_re.is_empty() {
            let idx = Self::select_pending(&mut self.pending_found_re, &mut self.inputs, to_mutate);
            self.update_avg_stats();
            idx
        } else {
            let v = rng.gen_range(1..=self.score_sum);
            match self.scores.binary_search(&v) {
                Ok(idx) => idx,
                Err(idx) => idx,
            }
        }
    }

    fn select_pending(f: &mut Vec<usize>, inputs: &mut [Input], to_mutate: bool) -> usize {
        if to_mutate {
            let idx = f.pop().unwrap();
            inputs[idx].was_mutated = true;
            idx
        } else {
            *f.choose_weighted_mut(&mut thread_rng(), |&idx| inputs[idx].score)
                .unwrap()
        }
    }

    pub fn append(&mut self, inp: Input) {
        let idx = self.inputs.len();
        self.append_inner(inp, idx);

        if let Some(stats) = self.stats.as_ref() {
            stats.update_time(stats::OVERALL_LAST_INPUT);
            stats.store(stats::OVERALL_CALLS_FUZZED_NUM, self.call_cnt.len() as u64);
        }

        if self.should_culling() {
            self.culling();
        }
        self.update_queue_stats();
    }

    fn append_inner(&mut self, inp: Input, idx: usize) {
        for c in &inp.p.calls {
            let cnt = self.call_cnt.entry(c.meta).or_default();
            *cnt += 1;
        }

        if inp.favored {
            self.favored.push(idx);
            if !inp.was_mutated {
                self.pending_favored.push(idx);
            }
        }

        if inp.found_new_re {
            self.found_re.push(idx);
            if !inp.was_mutated {
                self.pending_found_re.push(idx);
            }
        }

        self.score_sum += inp.score;
        self.scores.push(self.score_sum);

        // while inp.depth >= self.input_depth.len() {
        //     self.input_depth.push(Vec::new());
        // }
        // self.input_depth[inp.depth].push(idx);

        self.inputs.push(inp);
    }

    fn should_culling(&self) -> bool {
        let mut culling = false;
        if self.inputs.len() > self.last_num {
            culling = self.inputs.len() - self.last_num > self.culling_threshold;
        }
        if !culling {
            culling = (Instant::now() - self.last_culling) > self.culling_duration;
        }
        culling
    }

    fn culling(&mut self) {
        let now = Instant::now();
        log::info!(
            "Queue-{} starts culling, delta_len/threshold: {}/{}, last/duration: {:?}/{:?} (mins)",
            self.id,
            if self.inputs.len() > self.last_num {
                self.inputs.len() - self.last_num
            } else {
                0
            },
            self.culling_threshold,
            ((now - self.last_culling).as_secs()) / 60,
            self.culling_duration.as_secs() / 60
        );

        let mut inputs_old = std::mem::take(&mut self.inputs);
        inputs_old.sort_unstable_by(|i0, i1| {
            if i1.len != i0.len {
                i1.len.cmp(&i0.len)
            } else {
                i1.score.cmp(&i0.score)
            }
        });

        let mut cov = FxHashSet::default();
        let mut inputs = Vec::with_capacity(inputs_old.len());
        let mut discard = 0;
        let old_favored = self.favored.len();
        let mut new_favored = 0;
        for mut i in inputs_old.into_iter() {
            let mut favored = false;
            let mut new_cov = FxHashSet::default();

            // merge branches first, this could be very slow.
            for info in i.info.iter() {
                for br in info.branches.iter() {
                    if cov.insert(*br) {
                        favored = true;
                        new_cov.insert(*br);
                    }
                }
            }

            if !favored && i.len <= 2 && random::<bool>() {
                discard += 1;
                continue;
            }
            if favored {
                new_favored += 1;
            }
            i.new_cov = new_cov.into_iter().collect();
            i.new_cov.shrink_to_fit();
            i.favored = favored;
            i.age += 1;
            i.update_score();
            inputs.push(i);
        }

        inputs.shuffle(&mut thread_rng());

        let mut avgs = hashmap! {
            AVG_SZ => 0,
            AVG_AGE => 0,
            AVG_EXEC_TM => 0,
            AVG_RES_CNT => 0,
            AVG_NEW_COV => 0,
            AVG_LEN => 0,
            AVG_SCORE => 0
        };
        for i in inputs.iter_mut() {
            *avgs.get_mut(&AVG_AGE).unwrap() += i.age;
            // *avgs.get_mut(&AVG_SZ).unwrap() += i.sz;
            *avgs.get_mut(&AVG_LEN).unwrap() += i.len;
            *avgs.get_mut(&AVG_EXEC_TM).unwrap() += i.exec_tm;
            // *avgs.get_mut(&AVG_RES_CNT).unwrap() += i.res_cnt;
            *avgs.get_mut(&AVG_NEW_COV).unwrap() += i.new_cov.len();
            *avgs.get_mut(&AVG_SCORE).unwrap() += i.score;
        }
        avgs.iter_mut()
            .for_each(|(_, avg)| *avg = (*avg as f64 / inputs.len() as f64) as usize);

        let mut queue = Queue::new(self.id, self.out_dir.clone());
        queue.current_age = self.current_age + 1;
        queue.last_num = inputs.len();
        queue.last_culling = Instant::now();
        queue.avgs = avgs;
        for (idx, i) in inputs.into_iter().enumerate() {
            queue.append_inner(i, idx);
        }
        if let Some(stats) = self.stats.take() {
            stats.update_time(stats::QUEUE_LAST_CULLING);
            queue.set_stats(stats);
        }
        *self = queue;

        if let Some(queue_dir) = self.out_dir.as_ref() {
            if let Err(e) = self.dump(&queue_dir) {
                log::warn!("Queue-{}: failed to dump: {}", self.id, e);
            }
        }

        self.update_queue_stats();
        self.update_avg_stats();

        log::info!(
            "Queue-{} finished culling({}ms), age: {}, discard: {}, favored: {} -> {}, pending favored: {}",
            self.id,
            now.elapsed().as_millis(),
            self.current_age,
            discard,
            old_favored,
            new_favored,
            self.pending_favored.len()
        );
    }

    fn update_queue_stats(&self) {
        if let Some(stats) = self.stats.as_ref() {
            stats.store(stats::QUEUE_LEN, self.inputs.len() as u64);
            stats.store(stats::QUEUE_FAVOR, self.favored.len() as u64);
            stats.store(
                stats::QUEUE_PENDING_FAVOR,
                self.pending_favored.len() as u64,
            );
            stats.store(stats::QUEUE_SCORE, self.avgs[&AVG_SCORE] as u64);
            stats.store(stats::QUEUE_MAX_DEPTH, self.input_depth.len() as u64);
            stats.store(stats::QUEUE_AGE, self.current_age as u64);
        }
    }

    fn update_avg_stats(&self) {
        if let Some(stats) = self.stats.as_ref() {
            stats.store(stats::EXEC_AVG_SPEED, self.avgs[&AVG_EXEC_TM] as u64);
            stats.store(stats::AVG_LEN, self.avgs[&AVG_LEN] as u64);
            stats.store(stats::AVG_SZ, self.avgs[&AVG_SZ] as u64);
            stats.store(stats::AVG_AGE, self.avgs[&AVG_AGE] as u64);
            stats.store(stats::AVG_NEW_COV, self.avgs[&AVG_NEW_COV] as u64);
        }
    }

    pub fn dump<P: AsRef<Path>>(&self, out: P) -> Result<(), std::io::Error> {
        let queue_dir = out.as_ref().join(self.desciption());
        create_dir_all(&queue_dir)?;
        for inp in self.inputs.iter() {
            let inp_file = queue_dir.join(inp.desciption());
            write(inp_file, inp.p.to_string())?;
        }
        Ok(())
    }

    pub fn desciption(&self) -> String {
        let mut name = format!(
            "age:{},dep:{},calls:{},score:{},",
            self.current_age,
            self.input_depth.len(),
            self.call_cnt.len(),
            self.avgs[&AVG_SCORE]
        );
        if !self.favored.is_empty() {
            write!(name, "fav:{},", self.favored.len()).unwrap();
        }
        if !self.found_re.is_empty() {
            write!(name, "nre:{},", self.found_re.len()).unwrap();
        }

        name
    }
}
