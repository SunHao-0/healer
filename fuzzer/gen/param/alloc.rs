use bv::*;
use rand::prelude::*;
use rustc_hash::FxHashSet;

pub(crate) struct MemAlloc {
    bitmap: BitVec,
    last_idx: u64,
}

const ALLOC_GRANULE: u64 = 64;

impl MemAlloc {
    pub fn with_mem_size(sz: u64) -> Self {
        assert_eq!(sz % ALLOC_GRANULE, 0);
        Self {
            bitmap: BitVec::new_fill(false, sz / ALLOC_GRANULE),
            last_idx: 0,
        }
    }

    pub fn alloc(&mut self, mut sz: u64, mut align: u64) -> u64 {
        assert!(sz < self.bitmap.capacity() * ALLOC_GRANULE);

        if sz == 0 {
            sz = 1;
        }
        if align == 0 {
            align = 1;
        }

        let bit_size = (sz + ALLOC_GRANULE - 1) / ALLOC_GRANULE;
        let align_size = (align + ALLOC_GRANULE - 1) / ALLOC_GRANULE;
        if self.last_idx % align_size != 0 {
            self.last_idx += align_size - (self.last_idx % align_size);
        }

        let mut tried = 0;
        while tried != 2 {
            while self.last_idx + bit_size <= self.bitmap.capacity() {
                let mut bits = self
                    .bitmap
                    .as_mut_slice()
                    .bit_slice_mut(self.last_idx..self.last_idx + bit_size);
                if (0..bit_size).all(|i| !bits[i]) {
                    for i in 0..bit_size {
                        bits.set_bit(i, true);
                    }
                    let ret = self.last_idx;
                    self.last_idx += bit_size;
                    return ret * ALLOC_GRANULE;
                }
                self.last_idx += align_size;
            }
            self.last_idx = 0;
            tried += 1;
        }
        self.bitmap.clear();
        self.alloc(sz, align)
    }
}

pub(crate) struct VmaAlloc {
    page_num: u64,
    used: FxHashSet<u64>,
}

impl VmaAlloc {
    pub fn with_page_num(page_num: u64) -> Self {
        Self {
            page_num,
            used: FxHashSet::default(),
        }
    }

    pub fn alloc(&mut self, num: u64) -> u64 {
        let mut rng = thread_rng();
        let mut page;
        if self.used.is_empty() || rng.gen::<f32>() < 0.2 {
            page = rng.gen_range(0..4);
            if rng.gen::<f32>() < 0.01 {
                page = self.page_num - num - page;
            }
        } else {
            page = self.used.iter().copied().choose(&mut rng).unwrap();
            if num > 1 && rng.gen::<bool>() {
                let mut off = rng.gen_range(0..num);
                if off > page {
                    off = page
                }
                page -= off;
                if page + num > self.page_num {
                    page = self.page_num - num;
                }
            }
        }
        self.used.insert(page);
        page
    }
}
