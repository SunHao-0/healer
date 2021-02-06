use crate::{model::SyscallRef, targets::Target};

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, RwLock,
    },
};

use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;

pub struct Relation {
    relations: RwLock<FxHashMap<SyscallRef, FxHashSet<SyscallRef>>>,
    cnt: AtomicUsize,
    file: Option<Mutex<File>>,
}

impl Default for Relation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown system call '{name}', in line {line}")]
    UnknownSyscall { name: String, line: usize },
    #[error("bad format, in line {0}")]
    BadFormat(usize),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl Relation {
    pub fn new() -> Self {
        Self {
            relations: RwLock::new(FxHashMap::default()),
            file: None,
            cnt: AtomicUsize::new(0),
        }
    }

    pub fn with_workdir(dir: PathBuf) -> Result<Self, std::io::Error> {
        let f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(dir.join("relations"))?;
        Ok(Self {
            file: Some(Mutex::new(f)),
            ..Self::new()
        })
    }

    pub fn load<P: AsRef<Path>>(t: &Target, f: P) -> Result<Self, Error> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .append(true)
            .create(true)
            .open(f)?;
        let mut content = String::new();
        let mut r: FxHashMap<SyscallRef, FxHashSet<SyscallRef>> = FxHashMap::default();
        let mut cnt = 0;

        file.read_to_string(&mut content)?;
        for (n, l) in content.lines().enumerate().filter(|(_, l)| !l.is_empty()) {
            let calls = l.split(',').collect::<Vec<_>>();
            if calls.len() != 2 {
                return Err(Error::BadFormat(n));
            }
            let s0 = t
                .call_map
                .get(calls[0].trim())
                .ok_or_else(|| Error::UnknownSyscall {
                    name: String::from(calls[0].trim()),
                    line: n,
                })?;
            let s1 = t
                .call_map
                .get(calls[0].trim())
                .ok_or_else(|| Error::UnknownSyscall {
                    name: String::from(calls[0].trim()),
                    line: n,
                })?;
            let entry = r.entry(*s0).or_default();
            if entry.insert(*s1) {
                cnt += 1;
            }
        }

        Ok(Self {
            relations: RwLock::new(r),
            file: Some(Mutex::new(file)),
            cnt: AtomicUsize::new(cnt),
        })
    }

    pub fn insert(&self, s0: SyscallRef, s1: SyscallRef) -> Result<bool, std::io::Error> {
        let new;
        {
            let mut r = self.relations.write().unwrap();
            let enrty = r.entry(s0).or_default();
            new = enrty.insert(s1);
        }
        if new {
            self.cnt.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(f) = self.file.as_ref() {
            if new {
                let record = format!("\n{},{}", s0.name, s1.name);
                let mut f = f.lock().unwrap();
                f.write_all(record.as_bytes())?;
            }
        }
        Ok(new)
    }

    pub fn get(&self, s: SyscallRef) -> Option<FxHashSet<SyscallRef>> {
        let r = self.relations.read().unwrap();
        let mut ret = None;
        if let Some(calls) = r.get(&s) {
            ret = Some(calls.clone()); // TODO
        }
        ret
    }

    pub fn contains(&self, s0: SyscallRef, s1: SyscallRef) -> bool {
        let r = self.relations.read().unwrap();
        r.contains_key(&s0) && r[&s0].contains(&s1)
    }

    pub fn len(&self) -> usize {
        self.cnt.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
