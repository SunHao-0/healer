use healer_core::HashSet;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub struct Feedback {
    max_cov: RwLock<HashSet<u32>>,
    cal_cov: RwLock<HashSet<u32>>, // calibrated cover
}

impl Feedback {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_max_cov(&self, cov: impl IntoIterator<Item = u32>) -> HashSet<u32> {
        Self::check_inner(&self.max_cov, cov, true)
    }

    pub fn check_cal_cov(&self, cov: impl IntoIterator<Item = u32>) -> HashSet<u32> {
        Self::check_inner(&self.cal_cov, cov, false)
    }

    pub fn merge(&self, new: &HashSet<u32>) {
        Self::merge_inner(&self.max_cov, new);
        Self::merge_inner(&self.cal_cov, new);
    }

    pub fn max_cov_len(&self) -> usize {
        let m = self.max_cov.read().unwrap();
        m.len()
    }

    pub fn cal_cov_len(&self) -> usize {
        let m = self.cal_cov.read().unwrap();
        m.len()
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

        if !new.is_empty() && merge {
            Self::merge_inner(cov, &new);
        }

        new
    }

    fn merge_inner(cov: &RwLock<HashSet<u32>>, new: &HashSet<u32>) {
        let mut cov = cov.write().unwrap();
        cov.extend(new)
    }
}
