use crate::{fuzz::input::Input, model::SyscallRef};

use std::{
    mem,
    time::{Duration, Instant},
};

use iota::iota;
use rand::{prelude::*, random, thread_rng, Rng};
use rustc_hash::{FxHashMap, FxHashSet};

iota! {
    pub const AVG_GAINING_RATE: usize = iota;
        , AVG_DISTINCT_DEGREE
        , AVG_DEPTH
        , AVG_SZ
        , AVG_AGE
        , AVG_EXEC_TM
        , AVG_RES_CNT
        , AVG_NEW_COV
        , AVG_LEN
}

pub struct Queue {
    pub(crate) id: usize,
    pub(crate) inputs: Vec<Input>,

    pub(crate) current: usize,
    pub(crate) last_num: usize,
    pub(crate) last_culling: Instant,
    pub(crate) culling_threshold: usize,
    pub(crate) culling_duration: Duration,
    // stats of inputs.
    pub(crate) favored: Vec<usize>,
    pub(crate) pending_favored: Vec<usize>,
    pub(crate) pending_none_favored: Vec<usize>,
    pub(crate) found_re: Vec<usize>,
    pub(crate) pending_found_re: Vec<usize>,
    pub(crate) self_contained: Vec<usize>,
    pub(crate) score_sheet: Vec<(usize, usize)>, //socre, index
    pub(crate) min_score: (usize, usize),
    pub(crate) input_depth: Vec<Vec<usize>>,
    pub(crate) current_age: usize,
    pub(crate) avgs: FxHashMap<usize, usize>,
    pub(crate) call_cnt: FxHashMap<SyscallRef, usize>,
}

impl Queue {
    pub fn new(id: usize) -> Self {
        let avgs = fxhashmap! {
            AVG_GAINING_RATE => 0,
            AVG_DISTINCT_DEGREE => 0,
            AVG_DEPTH => 0,
            AVG_SZ => 0,
            AVG_AGE => 0,
            AVG_EXEC_TM => 0,
            AVG_RES_CNT => 0,
            AVG_NEW_COV => 0,
        };

        Self {
            id,
            last_culling: Instant::now(),
            inputs: Vec::new(),
            current: 0,
            last_num: 0,
            culling_threshold: 128,
            culling_duration: Duration::from_secs(30 * 60),
            favored: Vec::new(),
            pending_favored: Vec::new(),
            pending_none_favored: Vec::new(),
            found_re: Vec::new(),
            pending_found_re: Vec::new(),
            self_contained: Vec::new(),
            score_sheet: Vec::new(),
            min_score: (usize::MAX, 0),
            input_depth: Vec::new(),
            current_age: 0,
            avgs,
            call_cnt: FxHashMap::default(),
        }
    }

    pub fn select(&mut self, to_mutate: bool) -> &mut Input {
        let mut rng = thread_rng();

        // select pending
        let choose_weighted = |f: &mut Vec<usize>, inputs: &[Input]| {
            let idx = *f
                .choose_weighted_mut(&mut thread_rng(), |&idx| inputs[idx].score)
                .unwrap();
            let i = f.iter().position(|&i| i == idx).unwrap();
            if to_mutate {
                f.remove(i);
            }
            idx
        };

        if !self.pending_favored.is_empty() && rng.gen_range(1..=100) <= 90 {
            let idx = choose_weighted(&mut self.pending_favored, &self.inputs);
            return &mut self.inputs[idx];
        } else if !self.pending_found_re.is_empty() && rng.gen_range(1..=100) <= 60 {
            let idx = choose_weighted(&mut self.pending_found_re, &self.inputs);
            return &mut self.inputs[idx];
        } else if !self.pending_none_favored.is_empty() && rng.gen_range(1..=100) < 30 {
            let idx = choose_weighted(&mut self.pending_none_favored, &self.inputs);
            return &mut self.inputs[idx];
        };

        // select interesting
        const WINDOW_SZ: usize = 8;
        if !self.favored.is_empty() && rng.gen_range(1..=100) <= 50 {
            let idx = self.favored.choose(&mut rng).unwrap();
            return &mut self.inputs[*idx];
        } else if !self.found_re.is_empty() && rng.gen_range(1..=100) <= 30 {
            let idx = self.found_re.choose(&mut rng).unwrap();
            return &mut self.inputs[*idx];
        } else if !self.self_contained.is_empty() && rng.gen_range(1..=100) <= 10 {
            let idx = self.self_contained.choose(&mut rng).unwrap();
            return &mut self.inputs[*idx];
        } else if rng.gen_range(1..=100) <= 10 {
            let mut rng = thread_rng();
            let mut start = 0;
            let mut end = self.inputs.len();
            if self.inputs.len() > 8 {
                start = rng.gen_range(0..self.inputs.len() - WINDOW_SZ);
                end = start + WINDOW_SZ;
            }
            let (_, idx) = self.score_sheet[start..end]
                .choose_weighted(&mut rng, |(s, _)| *s)
                .unwrap();
            return &mut self.inputs[*idx];
        } else if rng.gen_range(1..=100) <= 2 {
            let idx = self.input_depth.last().unwrap().choose(&mut rng).unwrap();
            return &mut self.inputs[*idx];
        };

        // select weighted
        let start = self.current;
        let mut end = start + WINDOW_SZ;
        if end > self.inputs.len() {
            end = self.inputs.len();
        }
        self.current += 1;
        if self.current >= self.inputs.len() {
            self.current = 0;
        }
        (&mut self.inputs[start..end])
            .choose_weighted_mut(&mut thread_rng(), |i| i.score)
            .unwrap()
    }

    pub fn append(&mut self, inp: Input) {
        if self.should_culling() {
            self.culling();
        }
        let idx = self.inputs.len();
        self.append_inner(inp, idx);
    }

    fn append_inner(&mut self, inp: Input, idx: usize) {
        if inp.favored {
            self.favored.push(idx);
            if !inp.was_mutated {
                self.pending_favored.push(idx);
            }
        } else if !inp.was_mutated {
            self.pending_none_favored.push(idx);
        }
        if inp.found_new_re {
            self.found_re.push(idx);
            if !inp.was_mutated {
                self.pending_found_re.push(idx);
            }
        }
        if inp.self_contained {
            self.self_contained.push(idx);
        }
        self.score_sheet.push((inp.score, idx));
        if inp.score < self.min_score.0 {
            self.min_score = (inp.score, idx);
        }
        if inp.depth >= self.input_depth.len() {
            self.input_depth.push(Vec::new());
        }
        self.input_depth[inp.depth].push(idx);
        for c in &inp.p.calls {
            let cnt = self.call_cnt.entry(c.meta).or_default();
            *cnt += 1;
        }

        self.inputs.push(inp);
    }

    fn should_culling(&self) -> bool {
        if self.inputs.len() > self.last_num {
            // TODO update culling threshold based on execution speed dynamiclly
            let exceeds = self.inputs.len() - self.last_num > self.culling_threshold;
            // TODO update culling duration based on execution speed dynamiclly
            let tmout = self.last_culling.elapsed() > self.culling_duration;
            return exceeds || tmout;
        }
        false
    }

    fn culling(&mut self) {
        log::info!(
            "Queue{} starts culling, threshold/len: {}/{}, duration/last: {:?}/{:?}.",
            self.id,
            self.culling_threshold,
            self.inputs.len(),
            self.culling_duration,
            self.last_culling
        );
        let now = Instant::now();

        let mut inputs_old = mem::replace(&mut self.inputs, Vec::new());
        let old_len = inputs_old.len();
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
                        new_favored += 1;
                        new_cov.insert(*br);
                    }
                }
            }

            if !favored && i.len <= 2 && random::<bool>() {
                discard += 1;
                continue;
            }

            i.new_cov = new_cov.into_iter().collect();
            i.new_cov.shrink_to_fit();
            i.favored = favored;
            i.age += 1;
            inputs.push(i);
        }

        inputs.shuffle(&mut thread_rng());

        let mut avgs = fxhashmap! {
            AVG_GAINING_RATE => 0,
            AVG_DISTINCT_DEGREE => 0,
            AVG_DEPTH => 0,
            AVG_SZ => 0,
            AVG_AGE => 0,
            AVG_EXEC_TM => 0,
            AVG_RES_CNT => 0,
            AVG_NEW_COV => 0,
        };
        let mut call_cnt = FxHashMap::default();
        for i in inputs.iter() {
            for c in i.p.calls.iter() {
                let cnt = call_cnt.entry(c.meta).or_default();
                *cnt += 1;
            }
        }

        for i in inputs.iter_mut() {
            i.update_distinct_degree(&call_cnt);
            *avgs.get_mut(&AVG_GAINING_RATE).unwrap() += i.gaining_rate;
            *avgs.get_mut(&AVG_DISTINCT_DEGREE).unwrap() += i.distinct_degree;
            *avgs.get_mut(&AVG_AGE).unwrap() += i.age;
            *avgs.get_mut(&AVG_SZ).unwrap() += i.sz;
            *avgs.get_mut(&AVG_DEPTH).unwrap() += i.depth;
            *avgs.get_mut(&AVG_EXEC_TM).unwrap() += i.exec_tm;
            *avgs.get_mut(&AVG_RES_CNT).unwrap() += i.res_cnt;
            *avgs.get_mut(&AVG_NEW_COV).unwrap() += i.res_cnt;
        }
        avgs.iter_mut().for_each(|(_, avg)| *avg /= inputs.len());

        let mut queue = Queue::new(self.id);
        queue.call_cnt = call_cnt;
        queue.current_age = self.current_age + 1;
        queue.last_num = old_len;
        queue.last_culling = Instant::now();
        queue.culling_threshold = self.culling_threshold;
        queue.culling_duration = self.culling_duration;
        for (idx, mut i) in inputs.into_iter().enumerate() {
            i.update_score(&avgs);
            queue.append_inner(i, idx);
        }
        queue.avgs = avgs;

        *self = queue;
        log::info!(
            "Queue{} finished culling({}ms), age: {}, discard: {}, favored: {} -> {}, pending favored: {}",
            self.id,
            now.elapsed().as_millis(),
            self.current_age,
            discard,
            old_favored,
            new_favored,
            self.pending_favored.len()
        );
    }
}
