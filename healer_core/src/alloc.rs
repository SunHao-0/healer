//! A dummy allocator used for address allocation for ptr value without actual mem allocation.

use crate::{HashSet, RngType};
use core::alloc::Layout;
use rand::{prelude::SliceRandom, Rng};

pub type MemAddress = u64;
pub type MemSize = u64;

pub const ALLOC_GRANULE: u64 = 64; // in bytes
pub const DEFAULT_MEM_SIZE: u64 = 16 << 20;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Allocator {
    /// [(addr1, size1), (addr2, size2), ...] where addr1 > addr2
    free_blocks: Vec<(MemAddress, MemSize)>, // free blocks sorted by address
    /// index of free blocks, where size_i is max
    last_max: usize,
    /// total memory size to manage
    size: u64,
}

impl Default for Allocator {
    fn default() -> Self {
        Allocator::new(DEFAULT_MEM_SIZE)
    }
}

impl Allocator {
    pub fn new(sz: u64) -> Self {
        let layout = Layout::from_size_align(sz as usize, ALLOC_GRANULE as usize).unwrap();
        let layout = layout.pad_to_align();
        let sz = layout.size() as u64;
        Self {
            free_blocks: vec![(0, sz)],
            last_max: 0,
            size: sz,
        }
    }

    pub fn restore(&mut self) {
        *self = Self::new(self.size);
    }

    pub fn alloc(&mut self, layout: Layout) -> u64 {
        let layout = layout
            .align_to(ALLOC_GRANULE as usize)
            .unwrap()
            .pad_to_align();
        assert!(layout.size() < self.size as usize);

        // try to alloc with last max block
        if let Some(addr) = self.try_alloc(layout) {
            return addr;
        }

        // find new max block
        if self.update_max() {
            if let Some(addr) = self.try_alloc(layout) {
                return addr;
            }
        }

        // restart
        *self = Self::new(self.size);
        self.try_alloc(layout).unwrap()
    }

    fn try_alloc(&mut self, alloc_layout: Layout) -> Option<u64> {
        let (block_start, block_size) = self.free_blocks[self.last_max];
        let block_end = block_start + block_size;
        let block_layout = Layout::from_size_align(block_start as usize, alloc_layout.align())
            .unwrap()
            .pad_to_align();
        let aligned_addr = block_layout.size() as u64;
        let alloc_end = aligned_addr + alloc_layout.size() as u64;

        if alloc_end > block_end {
            return None;
        }

        if alloc_end + ALLOC_GRANULE > block_end {
            self.free_blocks.remove(self.last_max);
            self.update_max();
        } else {
            self.free_blocks[self.last_max] = (alloc_end, block_end - alloc_end);
        }

        Some(aligned_addr)
    }

    fn update_max(&mut self) -> bool {
        if self.free_blocks.is_empty() {
            *self = Self::new(self.size);
            return true;
        }

        let mut max = u64::MIN;
        let mut max_idx = 0;
        for (idx, &(_, sz)) in self.free_blocks.iter().enumerate() {
            if sz > max {
                max_idx = idx;
                max = sz;
            }
        }
        let updated = self.last_max != max_idx;
        self.last_max = max_idx;
        updated
    }

    #[allow(clippy::collapsible_else_if)]
    pub fn note_alloc(&mut self, alloc_addr: u64, alloc_sz: u64) -> bool {
        // for `alloc_addr`, free blocks [(a0, s0), (a1, s1), ...]
        // find if `alloc_addr` can be located in any block's [start_addr, end_addr)
        match self
            .free_blocks
            .binary_search_by(|(addr0, _)| addr0.cmp(&alloc_addr))
        {
            // match (a_i, s_i), a_i == alloc_addr
            Ok(idx) => {
                let (block_start, block_size) = self.free_blocks[idx];
                if block_size < alloc_sz {
                    return false;
                }
                if block_size < alloc_sz + ALLOC_GRANULE {
                    self.free_blocks.remove(idx);
                    self.update_max();
                } else {
                    self.free_blocks[idx] = (block_start + alloc_sz, block_size - alloc_sz);
                }
                true
            }
            // a_i > alloc_addr
            Err(idx) => {
                if idx != 0 {
                    let idx = idx - 1;
                    let (block_start, block_size) = self.free_blocks[idx];
                    let block_end = block_start + block_size;
                    let alloc_end = alloc_addr + alloc_sz;
                    if block_end < alloc_end {
                        return false;
                    }
                    if alloc_addr - block_start >= ALLOC_GRANULE {
                        self.free_blocks[idx] = (block_start, alloc_addr - block_start);
                        if block_end - alloc_end >= ALLOC_GRANULE {
                            self.free_blocks
                                .insert(idx + 1, (alloc_end, block_end - alloc_end));
                        }
                    } else {
                        if block_end - alloc_end >= ALLOC_GRANULE {
                            self.free_blocks[idx] = (alloc_end, block_end - alloc_end);
                        } else {
                            self.free_blocks.remove(idx);
                            self.update_max();
                        }
                    }
                    return true;
                }
                false
            }
        }
    }
}

/// Same vma allocator as Syzkaller
#[derive(Debug, Clone)]
pub struct VmaAllocator {
    page_num: u64,
    used: Vec<u64>,
    used_set: HashSet<u64>,
}

impl VmaAllocator {
    pub fn new(page_num: u64) -> Self {
        Self {
            page_num,
            used: Vec::new(),
            used_set: HashSet::default(),
        }
    }

    pub fn alloc(&mut self, rng: &mut RngType, num: u64) -> u64 {
        let mut page;
        if self.used.is_empty() || rng.gen_ratio(1, 5) {
            page = rng.gen_range(0..4);
            if !rng.gen_ratio(1, 100) {
                page = self.page_num - page - num;
            }
        } else {
            page = *self.used.choose(rng).unwrap();
            if num > 1 && rng.gen() {
                let mut off = rng.gen_range(0..num);
                if off > page {
                    off = page;
                }
                page -= off;
            }
            if page + num > self.page_num {
                page = self.page_num - num;
            }
        }
        self.note_alloc(page, num);
        page
    }

    pub fn note_alloc(&mut self, page_idx: u64, num: u64) {
        for page in page_idx..page_idx + num {
            if self.used_set.insert(page) {
                self.used.push(page);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_alloc() {
        let mut allocator = Allocator::new(1024);
        let addr = allocator.alloc(Layout::from_size_align(127, 1).unwrap());
        assert_eq!(addr, 0);
        let expected_free_block_addr = 128;
        let expected_free_block_size = 1024 - 128;
        assert_eq!(
            allocator.free_blocks,
            vec![(expected_free_block_addr, expected_free_block_size)]
        );

        let addr = allocator.alloc(Layout::from_size_align(32, 128).unwrap());
        assert_eq!(addr, 128);
        let expected_free_block_addr = expected_free_block_addr + 128;
        let expected_free_block_size = expected_free_block_size - 128;
        assert_eq!(
            allocator.free_blocks,
            vec![(expected_free_block_addr, expected_free_block_size)]
        );

        let addr = allocator.alloc(Layout::from_size_align(1024 - 256, 128).unwrap());
        assert_eq!(addr, 256);
        assert_eq!(allocator.free_blocks, vec![(0, 1024)])
    }

    #[test]
    fn allocator_note_alloc() {
        //
        let mut allocator = Allocator::new(1024);
        assert!(!allocator.note_alloc(1024, 128));

        assert!(allocator.note_alloc(512, 128));
        assert_eq!(
            allocator.free_blocks,
            vec![(0, 512), (512 + 128, 512 - 128)]
        );
        assert!(!allocator.note_alloc(512, 128));
        assert!(!allocator.note_alloc(1024, 128));
        assert!(allocator.note_alloc(0, 128));
    }
}
