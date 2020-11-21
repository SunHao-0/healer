use super::OsSpecificOperation;
use hlang::ast::Call;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) struct LinuxOperation;

impl OsSpecificOperation for LinuxOperation {
    fn make_data_mmap(&self) -> Box<[Call]> {
        // Add custom mmap gen
        todo!()
    }

    fn neutralize(&self, call: &Call) {
        todo!()
    }

    fn special_types(&self) -> FxHashMap<Box<str>, ()> {
        todo!()
    }

    fn aux_resources(&self) -> FxHashSet<Box<str>> {
        vec![
            "vma",
            "uid",
            "pid",
            "gid",
            "timespec",
            "timeval",
            "time_sec",
            "time_usec",
            "time_nsec",
        ]
        .into_iter()
        .map(|s| s.to_string().into_boxed_str())
        .collect()
    }

    fn special_ptr_val(&self) -> Box<[u64]> {
        vec![0xffffffff81000000].into_boxed_slice()
    }
}
