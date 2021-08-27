use healer_core::HashSet;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub struct Feedback {
    max_cov: RwLock<HashSet<u32>>,
    corpus_cov: RwLock<HashSet<u32>>,
}

impl Feedback {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_max_cov(&self, cov: impl IntoIterator<Item = u32>) -> HashSet<u32> {
        Self::check_inner(&self.max_cov, cov, true)
    }

    pub fn check_corpus_cov(&self, cov: impl IntoIterator<Item = u32>) -> HashSet<u32> {
        Self::check_inner(&self.corpus_cov, cov, false)
    }

    pub fn merge(&self, _cov: impl IntoIterator<Item = u32>) {
        todo!()
    }

    fn check_inner(
        cov: &RwLock<HashSet<u32>>,
        cov_in: impl IntoIterator<Item = u32>,
        merge: bool,
    ) -> HashSet<u32> {
        let mut new = HashSet::new();
        {
            let mc = cov.read().unwrap();
            for c in cov_in {
                if !mc.contains(&c) {
                    new.insert(c);
                }
            }
        }

        if merge && !new.is_empty() {
            let mut mv = cov.write().unwrap();
            for nc in new.iter() {
                mv.insert(*nc);
            }
        }

        new
    }
}
