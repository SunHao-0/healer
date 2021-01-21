use crate::{
    model::{SyscallRef, TypeRef},
    utils::to_boxed_str,
};
use rustc_hash::FxHashMap;

/// Syscall desciptions in json format of Syzkaller.
pub mod sys_json;
/// Parse syscalls json to ast, maintain internal static data.
mod syscalls;

/// Information of current test target.
pub struct Target {
    /// Name of target os.
    pub os: Box<str>,
    /// Target arch.
    pub arch: Box<str>,
    /// Revision of syscall description.
    pub revision: Box<str>,
    /// Ptr size of target arch.
    pub ptr_sz: u64,
    /// Page size of target os.
    pub page_sz: u64,
    /// Page number of target os.
    pub page_num: u64,
    /// Data offset of syz-executor.
    pub data_offset: u64,
    /// Endian of target arch.
    pub le_endian: bool,
    /// Use shared memory or not of syz-executor for current target.
    pub syz_exec_use_shm: bool,
    /// Use fork server or not of syz-executor for current target.
    pub syz_exec_use_forksrv: bool,
    /// Name of syz-executor binaray on target os.
    /// Equals to `Some`, when the target image already contains syz-executor.  
    pub syz_exec_bin: Option<Box<str>>,

    /// All syscalls of target os.
    pub syscalls: Box<[SyscallRef]>,
    /// All types of syscalls.
    pub tys: Box<[TypeRef]>,
    /// All resource types of `tys`.
    pub res_tys: Box<[TypeRef]>,
    /// All compatible resource types.
    pub res_eq_class: FxHashMap<TypeRef, Box<[TypeRef]>>,
}

impl Target {
    pub fn new<T: AsRef<str>>(target: T) -> Option<Self> {
        let target = target.as_ref();
        let desc_str = sys_json::load(target)?;
        let desc_json = json::parse(desc_str).unwrap();

        // let mut res_eq_class = Self::cal_res_eq_class(&res_tys);
        // assert!(res_tys.iter().all(|ty| ty.is_res_kind()));
        // Self::analyze_syscall_inout_res(&mut calls, &mut tys);
        // Self::complete_res_ty_info(&mut res_tys, &calls);
        // Self::filter_unreachable_res(&mut res_tys, &mut res_eq_class);

        let (syscalls, tys) = syscalls::parse(target, &desc_json);
        let res_tys = tys
            .iter()
            .copied()
            .filter(|ty| ty.res_desc().is_some())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let target_json = &desc_json["Target"];
        let os = target_json["OS"].as_str().unwrap();
        let arch = target_json["Arch"].as_str().unwrap();
        let revision = desc_json["Revision"].as_str().unwrap();
        let ptr_sz = target_json["PtrSize"].as_u64().unwrap();
        let page_sz = target_json["PageSize"].as_u64().unwrap();
        let page_num = target_json["NumPages"].as_u64().unwrap();
        let data_offset = target_json["DataOFfset"].as_u64().unwrap();
        let le_endian = target_json["LittleEndian"].as_bool().unwrap();
        let syz_exec_use_shm = target_json["ExecutorUsesShmem"].as_bool().unwrap();
        let syz_exec_use_forksrv = target_json["ExecutorUsesForkServer"].as_bool().unwrap();
        let syz_exec_bin = target_json["ExecutorBin"].as_str().unwrap();

        let target = Self {
            os: to_boxed_str(os),
            arch: to_boxed_str(arch),
            revision: to_boxed_str(revision),
            ptr_sz,
            page_sz,
            page_num,
            data_offset,
            le_endian,
            syz_exec_use_shm,
            syz_exec_use_forksrv,
            syz_exec_bin: if syz_exec_bin.is_empty() {
                None
            } else {
                Some(to_boxed_str(syz_exec_bin))
            },
            syscalls,
            tys,
            res_tys,
            res_eq_class: FxHashMap::default(), // TODO
        };
        Some(target)
    }

    pub fn physical_addr(&self, addr: u64) -> u64 {
        self.data_offset + addr
    }

    // fn is_equivalence_class(r1: TypeRef, r2: TypeRef) -> bool {
    //     let d1 = r1.res_desc().unwrap();
    //     let d2 = r2.res_desc().unwrap();
    //     let min_len = std::cmp::min(d1.kinds.len(), d2.kinds.len());
    //     (&d1.kinds[0..min_len])
    //         .iter()
    //         .eq((&d2.kinds[0..min_len]).iter())
    // }
}
