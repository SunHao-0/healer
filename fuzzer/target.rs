use hlang::ast::{Call, Dir, Syscall, Type, TypeId, TypeRef};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// Target maintain all information related to current test target.
pub struct Target {
    pub os: Box<str>,
    pub arch: Box<str>,
    pub revision: Box<str>,
    pub ptr_sz: u64,
    pub page_sz: u64,
    pub page_num: u64,
    pub data_offset: u64,
    pub le_endian: bool,

    /// All syscalls, order is important.
    pub syscalls: Vec<Rc<Syscall>>,
    /// All type of syscalls, order is important.
    pub tys: Vec<Rc<Type>>,
    // resource type is special, we maintain a dependent vec.
    pub res_tys: Vec<Rc<Type>>,
    /// Compatible resource type.
    pub res_eq_class: FxHashMap<Rc<Type>, Rc<[Rc<Type>]>>,

    // TODO
    pub os_specific_operation: Option<Box<dyn OsSpecificOperation>>,
    pub special_types: FxHashMap<Box<str>, ()>,
    pub aux_resources: FxHashSet<Box<str>>,
    pub special_ptr_val: Option<Box<[u64]>>,
}

impl Target {
    pub fn new(calls: Vec<Syscall>, mut tys: Vec<Rc<Type>>, re: &str) -> Self {
        let mut calls = calls.into_iter().map(Rc::new).collect::<Vec<_>>();

        Self::restore_typeref(&mut tys);
        let mut res_tys = tys
            .iter()
            .flat_map(|ty| Self::extract_res_ty(ty))
            .map(|(ty, _)| ty)
            .collect::<FxHashSet<Rc<Type>>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut res_eq_class = Self::cal_res_eq_class(&res_tys);
        assert!(res_tys.iter().all(|ty| ty.is_res_kind()));
        Self::analyze_syscall_inout_res(&mut calls, &mut tys);
        Self::complete_res_ty_info(&mut res_tys, &calls);
        Self::filter_unreachable_res(&mut res_tys, &mut res_eq_class);

        Target {
            os: syscalls::OS.to_string().into_boxed_str(),
            arch: syscalls::ARCH.to_string().into_boxed_str(),
            revision: syscalls::REVISION.to_string().into_boxed_str(),
            ptr_sz: syscalls::PTR_SIZE as u64,
            page_sz: syscalls::PARGE_SIZE as u64,
            page_num: syscalls::NUM_PARGS as u64,
            data_offset: syscalls::DATA_OFFSET as u64,
            le_endian: syscalls::LITTLE_ENDIAN,
            syscalls: calls,
            tys,
            res_tys,
            res_eq_class,
            os_specific_operation: Some(Box::new(linux::LinuxOperation)),
            special_types: FxHashMap::default(),
            aux_resources: FxHashSet::default(),
            special_ptr_val: None,
        }
    }

    pub fn physical_addr(&self, addr: u64) -> u64 {
        self.data_offset + addr
    }

    #[allow(clippy::collapsible_if)]
    fn filter_unreachable_res(
        res_tys: &mut Vec<Rc<Type>>,
        res_eq_class: &mut FxHashMap<Rc<Type>, Rc<[Rc<Type>]>>,
    ) {
        let mut reachable_res = FxHashSet::default();
        for (res, eq_res) in res_eq_class.iter() {
            if !reachable_res.contains(res) {
                if eq_res
                    .iter()
                    .any(|res| !res.res_desc().unwrap().ctors.is_empty())
                {
                    reachable_res.extend(eq_res.iter().cloned())
                }
            }
        }
        res_eq_class.retain(|k, _| reachable_res.contains(k));
        res_tys.retain(|r| reachable_res.contains(r));
        assert_eq!(res_eq_class.len(), res_tys.len());
    }

    /// Analyze input/output resources of eacho system call.
    /// Add these resources in call's input_res/output_res set.
    fn analyze_syscall_inout_res(scs: &mut [Rc<Syscall>], tys: &mut [Rc<Type>]) {
        for sc in scs.iter_mut() {
            let sc = Self::rc_to_mut(sc);
            // analyze each parameter first.
            let res_tys = sc
                .params
                .iter()
                .flat_map(|param| Self::extract_res_ty(&param.ty))
                .collect::<Vec<(Rc<Type>, Dir)>>();
            res_tys
                .into_iter()
                .for_each(|(res_ty, dir)| Self::record_syscall_res(sc, res_ty, dir));
            let ret_res_tys = sc.ret.as_ref().map(|res_ty| Self::extract_res_ty(res_ty));
            if let Some(res_tys) = ret_res_tys {
                res_tys
                    .into_iter()
                    .for_each(|(res_ty, _)| Self::record_syscall_res(sc, res_ty, Dir::Out));
            }
        }
    }

    fn record_syscall_res(sc: &mut Syscall, res_ty: Rc<Type>, dir: Dir) {
        let add_counter = |map: &mut FxHashMap<Rc<Type>, usize>, key: Rc<Type>| {
            let counter = map.entry(key).or_insert(0);
            *counter += 1;
        };

        match dir {
            Dir::In => add_counter(&mut sc.input_res, res_ty),
            Dir::Out => add_counter(&mut sc.output_res, res_ty),
            Dir::InOut => {
                add_counter(&mut sc.output_res, Rc::clone(&res_ty));
                add_counter(&mut sc.input_res, res_ty);
            }
        }
    }

    fn extract_res_ty(ty: &Rc<Type>) -> Vec<(Rc<Type>, Dir)> {
        let mut ctx = FxHashSet::default();
        let mut ret = Self::extract_res_ty_inner(ty, &mut ctx);
        ret.sort();
        ret.dedup();
        ret
    }

    fn extract_res_ty_inner(ty: &Rc<Type>, ctx: &mut FxHashSet<TypeId>) -> Vec<(Rc<Type>, Dir)> {
        use hlang::ast::TypeKind::*;
        if ctx.contains(&ty.id) {
            return Vec::new();
        } else {
            assert!(ctx.insert(ty.id));
        }
        match &(*ty).kind {
            Res { .. } => vec![(Rc::clone(ty), Dir::In)],
            Array { elem, .. } => Self::extract_res_ty_inner(elem.as_ref().unwrap(), ctx),
            Ptr { elem, dir } => Self::extract_res_ty_inner(elem.as_ref().unwrap(), ctx)
                .into_iter()
                .map(|(ty, _)| (ty, *dir))
                .collect::<Vec<_>>(),
            Struct { fields, .. } | Union { fields } => {
                let mut ret = Vec::new();
                for field in fields.iter() {
                    let mut res_tys = Self::extract_res_ty_inner(field.ty.as_ref().unwrap(), ctx);
                    res_tys
                        .iter_mut()
                        .for_each(|(_, dir)| *dir = field.dir.unwrap_or(Dir::Out));
                    ret.extend(res_tys);
                }
                ret
            }
            // for scalar type, just return empty vec.
            _ => Vec::new(), // empty vec,
        }
    }

    /// Complete resource type info, such as constructors and consumers.
    fn complete_res_ty_info(res_tys: &mut [Rc<Type>], syscalls: &[Rc<Syscall>]) {
        for res_ty in res_tys.iter_mut() {
            let res_ty = Self::rc_to_mut(res_ty);
            for sc in syscalls {
                if sc.output_res.contains_key(res_ty) {
                    res_ty.res_desc_mut().unwrap().ctors.insert(Rc::clone(sc));
                }
                if sc.input_res.contains_key(res_ty) {
                    res_ty
                        .res_desc_mut()
                        .unwrap()
                        .consumers
                        .insert(Rc::clone(sc));
                }
            }
        }
    }

    /// Restore typeref value from id to ref.
    fn restore_typeref(tys: &mut [Rc<Type>]) {
        use hlang::ast::TypeKind::*;

        for i in 0..tys.len() {
            // This is necessary to pass rustc borrow checker.
            let mut ty = Rc::clone(&tys[i]);
            match &mut Self::rc_to_mut(&mut ty).kind {
                Array { elem, .. } | Ptr { elem, .. } => {
                    *elem = TypeRef::Ref(Rc::clone(&tys[elem.as_id().unwrap()]));
                }
                Struct { fields, .. } | Union { fields, .. } => {
                    for field in fields.iter_mut() {
                        field.ty = TypeRef::Ref(Rc::clone(&tys[field.ty.as_id().unwrap()]));
                    }
                }
                // just pass other ty kinds
                _ => (),
            }
        }
    }

    #[allow(clippy::transmute_ptr_to_ref)]
    fn rc_to_mut<T>(rc: &mut Rc<T>) -> &mut T {
        use std::mem::transmute;
        // Safety, only used during constructing target and all methods guarantee the safe use of ref.
        // After construction, the target inmutable.
        unsafe { transmute(Rc::as_ptr(rc)) }
    }

    /// Calculate equivalence class of resource type
    fn cal_res_eq_class(res_tys: &[Rc<Type>]) -> FxHashMap<Rc<Type>, Rc<[Rc<Type>]>> {
        let mut ret = FxHashMap::default();
        let mut left_res_tys = Vec::from(res_tys);
        loop {
            if left_res_tys.is_empty() {
                break;
            }

            let ty1 = left_res_tys.pop().unwrap(); // so, the loop will stop.
            let mut eq_class = FxHashSet::default();
            for ty2 in left_res_tys.iter() {
                if Self::is_equivalence_class(&ty1, ty2) {
                    eq_class.insert(Rc::clone(ty2));
                }
            }

            left_res_tys.retain(|x| !eq_class.contains(x));
            eq_class.insert(ty1);

            let eq_class: Rc<[Rc<Type>]> =
                Rc::from(eq_class.into_iter().collect::<Vec<Rc<Type>>>());

            for ty in (*eq_class).iter() {
                ret.insert(Rc::clone(ty), Rc::clone(&eq_class));
            }
        }
        assert_eq!(ret.len(), res_tys.len());
        assert!(res_tys.iter().all(|ty| ret.contains_key(ty)));
        ret
    }

    fn is_equivalence_class(r1: &Rc<Type>, r2: &Rc<Type>) -> bool {
        let d1 = r1.res_desc().unwrap();
        let d2 = r2.res_desc().unwrap();
        let min_len = std::cmp::min(d1.kinds.len(), d2.kinds.len());
        (&d1.kinds[0..min_len])
            .iter()
            .eq((&d2.kinds[0..min_len]).iter())
    }
}

pub trait OsSpecificOperation {
    fn make_data_mmap(&self) -> Box<[Call]>;
    fn neutralize(&self, call: &Call);
    fn special_types(&self) -> FxHashMap<Box<str>, ()>;
    fn aux_resources(&self) -> FxHashSet<Box<str>>;
    fn special_ptr_val(&self) -> Box<[u64]>;
}
mod linux {

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
}
