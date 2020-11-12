#[cfg(feature = "amd64-linux")]
#[path = "syscalls/linux/amd64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "386-linux")]
#[path = "syscalls/linux/_386.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "arm-linux")]
#[path = "syscalls/linux/arm.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "arm64-linux")]
#[path = "syscalls/linux/arm64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "mips64le-linux")]
#[path = "syscalls/linux/mips64le.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "ppc64le-linux")]
#[path = "syscalls/linux/ppc64le.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "riscv64-linux")]
#[path = "syscalls/linux/riscv64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "s390x-linux")]
#[path = "syscalls/linux/s390x.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "amd64-akaros")]
#[path = "syscalls/akaros/amd64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "_386-freebsd")]
#[path = "syscalls/freebsd/_386.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "amd64-freebsd")]
#[path = "syscalls/freebsd/amd64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "amd64-fuchsia")]
#[path = "syscalls/fuchsia/amd64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "arm64-fuchsia")]
#[path = "syscalls/fuchsia/arm64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "amd64-netbsd")]
#[path = "syscalls/netbsd/amd64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "amd64-openbsd")]
#[path = "syscalls/openbsd/amd64.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "arm-trusty")]
#[path = "syscalls/trusty/arm.rs"]
#[rustfmt::skip]mod syscalls;

#[cfg(feature = "amd64-windows")]
#[path = "syscalls/windows/amd64.rs"]
#[rustfmt::skip]mod syscalls;

mod linux;

use hlang::ast::{Call, Syscall, Type};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// A test target
pub struct Target {
    pub os: Box<str>,
    pub arch: Box<str>,
    pub revision: Box<str>,
    pub ptr_sz: u64,
    pub page_sz: u64,
    pub page_num: u64,
    pub data_offset: u64,
    pub le_endian: bool,

    pub syscalls: Vec<Rc<Syscall>>,
    pub tys: Vec<Rc<Type>>,
    pub res: Vec<Rc<Type>>,

    pub gen_res_calls: FxHashMap<Box<str>, Box<[Rc<Syscall>]>>,
    pub consume_res_call: FxHashMap<Box<str>, Box<[Rc<Syscall>]>>,
    pub compatible_res: FxHashMap<Box<str>, Box<[Box<str>]>>,
    pub res_fuzz_count: FxHashMap<Box<str>, u64>,
    pub call_fuzz_count: FxHashMap<Rc<Syscall>, u64>,

    pub os_specific_operation: Option<Box<dyn OsSpecificOperation>>,
    pub special_types: FxHashMap<Box<str>, ()>,
    pub aux_resources: FxHashSet<Box<str>>,
    pub special_ptr_val: Box<[u64]>,
}

impl Target {
    pub fn new() -> Self {
        let _re = syscalls::REVISION;
        let (_syscalls, _tys) = syscalls::syscalls();
        let _ = Target::cal_gen_res_calls(&_syscalls);
        let _ = Target::cal_consume_res_call(&_syscalls);
        let _ = Target::cal_compatible_res(&_tys);
        let _: Option<Box<dyn OsSpecificOperation>> = if cfg!(features = "amd64-linux") {;
            Some(Box::new(linux::LinuxOperation::new()))
        }else{
            None
        };

        todo!()
    }

    fn cal_gen_res_calls(calls: &[Syscall]) -> FxHashMap<Box<str>, Rc<Syscall>> {
        todo!()
    }

    fn cal_consume_res_call(calls: &[Syscall]) -> FxHashMap<Box<str>, Box<[Rc<Syscall>]>> {
        todo!()
    }

    fn cal_compatible_res(res: &[Rc<Type>]) -> FxHashMap<Box<str>, Box<[Box<str>]>> {
        todo!()
    }
}

pub trait OsSpecificOperation {
    fn make_data_mmap(&self) -> Box<[Call]>;
    fn neutralize(&self, call: &Call);
    fn special_types(&self) -> FxHashMap<Box<str>, ()>;
    fn aux_resources(&self) -> FxHashSet<Box<str>>;
    fn special_ptr_val(&self) -> Box<[u64]>;
}
