use crate::{gen::choose_weighted, prog::Prog, HashMap, RngType};
use std::sync::RwLock;

#[derive(Debug)]
pub struct CorpusWrapper {
    pub inner: RwLock<Corpus>,
}

impl Default for CorpusWrapper {
    fn default() -> Self {
        Self {
            inner: RwLock::new(Corpus::default()),
        }
    }
}

impl CorpusWrapper {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Corpus::new()),
        }
    }

    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.len()
    }

    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.is_empty()
    }

    pub fn add_prog(&self, p: Prog, prio: u64) -> CorpusId {
        let mut inner = self.inner.write().unwrap();
        inner.add_prog(p, prio)
    }

    pub fn select_one(&self, rng: &mut RngType) -> Option<Prog> {
        let inner = self.inner.read().unwrap();
        inner.select_one(rng).cloned()
    }

    pub fn culling<F>(&self, f: F) -> usize
    where
        F: FnMut(&mut ProgInfo),
    {
        let mut inner = self.inner.write().unwrap();
        inner.culling(f)
    }
}

pub type CorpusId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgInfo {
    pub id: CorpusId,
    pub prog: Prog,
    pub prio: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Corpus {
    progs: Vec<ProgInfo>,
    id_to_index: HashMap<CorpusId, usize>,
    next_id: CorpusId,
    prios: Vec<u64>,
    sum_prios: u64,
}

impl Corpus {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.progs.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.progs.is_empty()
    }

    fn next_id(&mut self) -> CorpusId {
        let ret = self.next_id;
        self.next_id += 1;
        ret
    }

    pub fn add_prog(&mut self, prog: Prog, prio: u64) -> CorpusId {
        debug_assert_ne!(prio, 0);
        let id = self.next_id();
        self.add_prog_with_id(id, prog, prio);

        id
    }

    fn add_prog_with_id(&mut self, id: CorpusId, prog: Prog, prio: u64) {
        self.sum_prios += prio;
        self.prios.push(self.sum_prios);
        let p = ProgInfo { id, prog, prio };
        let idx = self.progs.len();
        self.progs.push(p);
        self.id_to_index.insert(id, idx);
    }

    pub fn get(&self, id: usize) -> Option<&Prog> {
        let idx = self.id_to_index.get(&id)?;
        self.progs.get(*idx).map(|p| &p.prog)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Prog> {
        let idx = self.id_to_index.get(&id)?;
        self.progs.get_mut(*idx).map(|p| &mut p.prog)
    }

    pub fn select_one(&self, rng: &mut RngType) -> Option<&Prog> {
        if !self.is_empty() {
            let idx = choose_weighted(rng, &self.prios);
            Some(&self.progs[idx].prog)
        } else {
            None
        }
    }

    pub fn culling<F>(&mut self, mut update: F) -> usize
    where
        F: FnMut(&mut ProgInfo),
    {
        let mut new_corpus = Corpus {
            progs: Vec::with_capacity(self.len()),
            id_to_index: HashMap::with_capacity(self.len()),
            next_id: self.next_id, // keep the old id count
            prios: Vec::with_capacity(self.len()),
            sum_prios: 0,
        };

        let mut n = 0;
        let progs = std::mem::take(&mut self.progs);
        for mut p in progs {
            update(&mut p);
            if p.prio != 0 {
                n += 1;
                new_corpus.add_prog_with_id(p.id, p.prog, p.prio);
            }
        }
        *self = new_corpus;
        n
    }
}
