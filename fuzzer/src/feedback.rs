use executor::Branch;
use std::collections::HashSet;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct FeedBack {
    branches: Mutex<HashSet<Branch>>,
}

impl FeedBack {
    pub async fn merge(&self, branches: Vec<Branch>) -> Option<Vec<Branch>> {
        let mut inner = self.branches.lock().await;

        let mut new_branches = Vec::new();
        for b in branches.into_iter() {
            if !inner.contains(&b) {
                new_branches.push(b.clone());
                inner.insert(b);
            }
        }
        if new_branches.is_empty() {
            None
        } else {
            Some(new_branches)
        }
    }

    pub async fn len(&self) -> usize {
        let b = self.branches.lock().await;
        b.len()
    }

    pub async fn is_empty(&self) -> bool {
        let b = self.branches.lock().await;
        b.is_empty()
    }
}
