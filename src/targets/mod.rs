use crate::{
    model::{SyscallRef, TypeRef},
    utils::to_boxed_str,
};
use rustc_hash::{FxHashMap, FxHashSet};

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
    /// All types of `syscalls`.
    pub tys: Box<[TypeRef]>,
    /// All resource types of `tys`.
    pub res_tys: Box<[TypeRef]>,
    /// Subtypes of each resource type.
    pub subtype_map: FxHashMap<TypeRef, Box<[TypeRef]>>,
    /// Supertypes of each resource type.
    pub supertype_map: FxHashMap<TypeRef, Box<[TypeRef]>>,
}

impl Target {
    pub fn new<T: AsRef<str>>(target: T) -> Option<Self> {
        let target = target.as_ref();
        let desc_str = sys_json::load(target)?;
        let desc_json = json::parse(desc_str).unwrap();

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
        let data_offset = target_json["DataOffset"].as_u64().unwrap();
        let le_endian = target_json["LittleEndian"].as_bool().unwrap();
        let syz_exec_use_shm = target_json["ExecutorUsesShmem"].as_bool().unwrap();
        let syz_exec_use_forksrv = target_json["ExecutorUsesForkServer"].as_bool().unwrap();
        let syz_exec_bin = target_json["ExecutorBin"].as_str().unwrap();
        let mut subtype_map = FxHashMap::default();
        let mut supertype_map = FxHashMap::default();
        for r0 in res_tys.iter().copied() {
            let mut subtypes = FxHashSet::default();
            let mut supertypes = FxHashSet::default();
            for r1 in res_tys.iter().copied() {
                if Self::is_subtype(r0, r1) {
                    subtypes.insert(r1);
                } else if Self::is_supertype(r0, r1) {
                    supertypes.insert(r1);
                }
            }
            subtype_map.insert(
                r0,
                subtypes.into_iter().collect::<Vec<_>>().into_boxed_slice(),
            );
            supertype_map.insert(
                r0,
                supertypes
                    .into_iter()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            );
        }

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
            subtype_map,
            supertype_map,
        };
        Some(target)
    }

    pub fn physical_addr(&self, addr: u64) -> u64 {
        self.data_offset + addr
    }

    fn is_subtype(dst: TypeRef, src: TypeRef) -> bool {
        let dst_desc = dst.res_desc().unwrap();
        let src_desc = src.res_desc().unwrap();
        if dst_desc.kinds.len() < src_desc.kinds.len() {
            *dst_desc.kinds == src_desc.kinds[0..dst_desc.kinds.len()]
        } else {
            false
        }
    }

    fn is_supertype(dst: TypeRef, src: TypeRef) -> bool {
        let dst_desc = dst.res_desc().unwrap();
        let src_desc = src.res_desc().unwrap();
        if dst_desc.kinds.len() > src_desc.kinds.len() {
            dst_desc.kinds[0..src_desc.kinds.len()] == *src_desc.kinds
        } else {
            false
        }
    }
}
