use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    mem,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread::sleep,
    time::{Duration, Instant},
};

use iota::iota;
use rustc_hash::FxHashMap;

iota! {
            // Overall
    pub const OVERALL_RUN_TIME: u64 = iota;
            , OVERALL_LAST_INPUT
            , OVERALL_MAX_COV
            , OVERALL_CAL_COV
            , OVERALL_LAST_CRASH
            , OVERALL_TOTAL_CRASHES
            , OVERALL_UNIQUE_CRASHES
            , OVERALL_CALLS_FUZZED_NUM
            // Exec
            , EXEC_AVG_SPEED
            , EXEC_EXEC_ALL
            , EXEC_GEN
            , EXEC_MUTATION
            , EXEC_MINIMIZE
            , EXEC_RDETECT
            , EXEC_CALIBRATE
            // Queue
            , QUEUE_LAST_CULLING        // last culling time.
            , QUEUE_LEN                 // (now/last).
            , QUEUE_FAVOR               // number of favored
            , QUEUE_PENDING_FAVOR
            , QUEUE_SCORE               // (max/min)
            , QUEUE_SELF_CONTAIN
            , QUEUE_MAX_DEPTH
            , QUEUE_AGE
            // Average stats of inputs.
            , AVG_LEN
            , AVG_GAINNING
            , AVG_DIST
            , AVG_DEPTH
            , AVG_SZ
            , AVG_AGE
            , AVG_NEW_COV

            , STATS_LEN // place holder.
}

lazy_static! {
    pub static ref STATS: FxHashMap<u64, &'static str> = {
        fxhashmap! {
            OVERALL_RUN_TIME            => "run time",
            OVERALL_LAST_INPUT          => "last input",
            OVERALL_MAX_COV             => "max cov",
            OVERALL_CAL_COV             => "cal cov",
            OVERALL_LAST_CRASH          => "last crash",
            OVERALL_TOTAL_CRASHES       => "total crashes",
            OVERALL_UNIQUE_CRASHES      => "uniq crashes",
            OVERALL_CALLS_FUZZED_NUM    => "call fuzzed",
            EXEC_AVG_SPEED              => "exec speed",
            EXEC_EXEC_ALL               => "exec all",
            EXEC_GEN                    => "exec gen",
            EXEC_MUTATION               => "exec_mut",
            EXEC_MINIMIZE               => "exec mini",
            EXEC_RDETECT                => "exec dect",
            EXEC_CALIBRATE              => "exec cal",
            QUEUE_LAST_CULLING          => "last culling",
            QUEUE_LEN                   => "length",
            QUEUE_FAVOR                 => "favored",
            QUEUE_PENDING_FAVOR         => "pending fav",
            QUEUE_SCORE                 => "score",
            QUEUE_SELF_CONTAIN          => "self contain",
            QUEUE_MAX_DEPTH             => "depth",
            QUEUE_AGE                   => "age",
            AVG_LEN                     => "prog len",
            AVG_GAINNING                => "gain rate",
            AVG_DIST                    => "dist",
            AVG_DEPTH                   => "avg depth",
            AVG_SZ                      => "prog size",
            AVG_AGE                     => "avg age",
            AVG_NEW_COV                 => "new cov"
        }
    };
    pub static ref GROUPS: FxHashMap<&'static str, Vec<u64>> = {
        fxhashmap! {
            "OVERALL" => vec![
                OVERALL_RUN_TIME,
                OVERALL_LAST_INPUT,
                OVERALL_MAX_COV,
                OVERALL_CAL_COV,
                OVERALL_LAST_CRASH,
                OVERALL_TOTAL_CRASHES,
                OVERALL_UNIQUE_CRASHES,
                OVERALL_CALLS_FUZZED_NUM
            ],
            "EXEC" => vec![
                EXEC_AVG_SPEED,
                EXEC_EXEC_ALL,
                EXEC_GEN,
                EXEC_MUTATION,
                EXEC_MINIMIZE,
                EXEC_RDETECT,
                EXEC_CALIBRATE
            ],
            "QUEUE" => vec![
                QUEUE_LAST_CULLING,
                QUEUE_LEN,
                QUEUE_FAVOR,
                QUEUE_PENDING_FAVOR,
                QUEUE_SCORE,
                QUEUE_SELF_CONTAIN,
                QUEUE_MAX_DEPTH,
                QUEUE_AGE
            ],
            "AVERAGE" => vec![
                AVG_LEN,
                AVG_GAINNING,
                AVG_DIST,
                AVG_DEPTH,
                AVG_SZ,
                AVG_AGE,
                AVG_NEW_COV
            ]
        }
    };
}

pub struct Stats {
    start_tm: Instant,
    stats: [AtomicU64; STATS_LEN as usize],
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}

impl Stats {
    pub fn new() -> Self {
        let s = [0u64; STATS_LEN as usize];
        Self {
            start_tm: Instant::now(),
            // SAFETY: AtomicU64 has the same in-memory representation as u64.
            stats: unsafe { mem::transmute(s) },
        }
    }

    pub fn inc(&self, stat: u64) -> u64 {
        self.stats[stat as usize].fetch_add(1, Ordering::Relaxed)
    }

    pub fn inc_exec(&self, stat: u64) -> u64 {
        if stat != EXEC_EXEC_ALL {
            self.inc(EXEC_EXEC_ALL);
        }
        self.inc(stat)
    }

    pub fn add(&self, stat: u64, n: u64) -> u64 {
        self.stats[stat as usize].fetch_add(n, Ordering::Relaxed)
    }

    pub fn load(&self, stat: u64) -> u64 {
        self.stats[stat as usize].load(Ordering::Relaxed)
    }

    pub fn store(&self, stat: u64, val: u64) {
        self.stats[stat as usize].store(val, Ordering::Relaxed);
    }

    pub fn update_time(&self, stat: u64) {
        let d = self.start_tm.elapsed();
        let d = d.as_millis() as u64;
        self.stats[stat as usize].store(d, Ordering::Relaxed);
    }

    pub fn to_json_str(&self) -> String {
        let mut grouped = HashMap::new(); // json crate can not stringify FxHashMap.
        for (&group_name, keys) in GROUPS.iter() {
            let mut sub_vals = HashMap::new();
            for key in keys.iter().copied() {
                sub_vals.insert(STATS[&key], self.load(key));
            }
            grouped.insert(group_name, sub_vals);
        }
        json::stringify_pretty(grouped, 4)
    }
}

pub fn bench(du: Duration, work_dir: PathBuf, stats: Arc<Stats>) {
    let mut stats_file = File::create(work_dir.join("stats.json")).unwrap();
    loop {
        sleep(du);
        let stats_json = stats.to_json_str();
        log::info!("========== Stats ==========\n{}", stats_json);
        stats_file.write_all(stats_json.as_bytes()).unwrap();
    }
}
