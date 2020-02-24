use std::collections::HashSet;
use std::iter::Extend;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Default, Hash, PartialOrd, PartialEq, Ord, Eq)]
pub struct Block(usize);

impl From<usize> for Block {
    fn from(raw: usize) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Debug, Default, Hash, PartialOrd, PartialEq, Ord, Eq)]
pub struct Branch(usize);

impl From<(Block, Block)> for Branch {
    fn from((b1, b2): (Block, Block)) -> Self {
        let b1 = b1.0 >> 1;
        Self(b1 ^ b2.0)
    }
}

#[derive(Default)]
pub struct FeedBack {
    branches: Mutex<HashSet<Branch>>,
    blocks: Mutex<HashSet<Block>>,
}

impl FeedBack {
    pub async fn diff_branch(&self, branches: &[Branch]) -> HashSet<Branch> {
        let inner = self.branches.lock().await;

        let mut result = HashSet::new();
        for b in branches {
            if !inner.contains(b) {
                result.insert(b.clone());
            }
        }
        result.shrink_to_fit();
        result
    }

    pub async fn diff_block(&self, blocks: &[Block]) -> HashSet<Block> {
        let inner = self.blocks.lock().await;

        let mut result = HashSet::new();
        for b in blocks {
            if !inner.contains(b) {
                result.insert(b.clone());
            }
        }
        result.shrink_to_fit();
        result
    }

    pub async fn merge(&self, blocks: HashSet<Block>, branches: HashSet<Branch>) {
        {
            let mut inner = self.branches.lock().await;
            inner.extend(branches);
        }
        {
            let mut inner = self.blocks.lock().await;
            inner.extend(blocks);
        }
    }

    pub async fn branch_len(&self) -> usize {
        let b = self.branches.lock().await;
        b.len()
    }

    pub async fn block_len(&self) -> usize {
        let b = self.blocks.lock().await;
        b.len()
    }
}
