use core::prog::Prog;
use std::collections::HashSet;
use std::iter::FromIterator;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
pub struct Corpus {
    inner: Mutex<HashSet<Prog>>,
}

impl Corpus {
    pub async fn insert(&self, p: Prog) -> bool {
        let mut inner = self.inner.lock().await;
        inner.insert(p)
    }

    pub async fn len(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.len()
    }

    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.is_empty()
    }

    pub async fn dump(self) -> bincode::Result<Vec<u8>> {
        let inner = self.inner.lock().await;
        let mut progs = inner
            .iter()
            .map(|p| {
                let mut p = p.clone();
                p.shrink();
                p
            })
            .collect::<Vec<_>>();
        progs.shrink_to_fit();
        bincode::serialize(&progs)
    }

    pub fn load(c: &[u8]) -> bincode::Result<Self> {
        let mut progs: Vec<Prog> = bincode::deserialize(c)?;
        progs.shrink_to_fit();
        Ok(Self {
            inner: Mutex::new(HashSet::from_iter(progs)),
        })
    }
}
