use super::OsSpecificOperation;
use hlang::ast::Call;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) struct LinuxOperation;

impl LinuxOperation {
    pub fn new() -> Self {
        todo!()
    }
}

impl OsSpecificOperation for LinuxOperation {
    fn make_data_mmap(&self) -> Box<[Call]> {
        todo!()
    }
    fn neutralize(&self, call: &Call) {
        todo!()
    }
    fn special_types(&self) -> FxHashMap<Box<str>, ()> {
        todo!()
    }
    fn aux_resources(&self) -> FxHashSet<Box<str>> {
        todo!()
    }
    fn special_ptr_val(&self) -> Box<[u64]> {
        todo!()
    }
}
