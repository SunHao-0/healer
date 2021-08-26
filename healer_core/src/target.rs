use crate::{
    syscall::{Syscall, SyscallId},
    ty::{Dir, ResKind, ResType, Type, TypeId, TypeKind},
    HashMap, HashSet,
};

#[derive(Debug, Clone)]
pub struct Target {
    /// Name of target os.
    os: Box<str>,
    /// Target arch.
    arch: Box<str>,
    /// Ptr size of target arch.
    ptr_sz: u64,
    /// Page size of target os.
    page_sz: u64,
    /// Page number of target os.
    page_num: u64,
    /// Endian of target arch.
    le_endian: bool,
    /// Special pointer values.
    special_ptrs: Box<[u64]>,
    // TODO This really should not be here, it's highly related to syz-executor
    // move it to syz-wrappers
    /// executor data offset
    data_offset: u64,

    /// Syzlang description revision.
    revision: Box<str>,
    /// All syscalls of current os, sorted by `SyscallId`.
    all_syscalls: Vec<Syscall>,
    /// Enable syscalls of current target.
    enabled_syscalls: Vec<Syscall>,
    /// All types, sorted by `TypeId`.
    tys: Vec<Type>,
    /// Syscall id to syscall mapping.
    syscall_id_mapping: HashMap<SyscallId, Syscall>,
    /// Type id to type mapping.
    ty_id_mapping: HashMap<TypeId, Type>,
    /// Syscall name to syscall id mapping.
    syscall_name_mapping: HashMap<Box<str>, SyscallId>,
    /// All resource type, sorted by `TypeId`.
    res_tys: Vec<ResType>,
    /// All resource kind.
    res_kinds: Vec<ResKind>,
    /// Resource name to resource type id mapping.
    res_name_mapping: HashMap<ResKind, Vec<TypeId>>,
    /// Super types of each resource type.
    res_super_ty_mapping: HashMap<ResKind, Vec<ResKind>>,
    /// Sub type of each resource type.
    res_sub_ty_mapping: HashMap<ResKind, Vec<ResKind>>,
    /// Input resource types of each syscall.
    syscall_input_res: HashMap<SyscallId, Vec<ResKind>>,
    /// Output resource types of each syscall.
    syscall_output_res: HashMap<SyscallId, Vec<ResKind>>,
    /// Syscalls that use the resource as input.
    res_input_syscall: HashMap<ResKind, Vec<SyscallId>>,
    /// Syscalls that can output the resource.
    res_output_syscall: HashMap<ResKind, Vec<SyscallId>>,
}

impl Target {
    #[inline(always)]
    pub fn os(&self) -> &str {
        &self.os
    }

    #[inline(always)]
    pub fn arch(&self) -> &str {
        &self.arch
    }

    #[inline(always)]
    pub fn ptr_sz(&self) -> u64 {
        self.ptr_sz
    }

    #[inline(always)]
    pub fn page_num(&self) -> u64 {
        self.page_num
    }

    #[inline(always)]
    pub fn page_sz(&self) -> u64 {
        self.page_sz
    }

    #[inline(always)]
    pub fn le_endian(&self) -> bool {
        self.le_endian
    }

    #[inline(always)]
    pub fn special_ptrs(&self) -> &[u64] {
        &self.special_ptrs
    }

    #[inline(always)]
    pub fn data_offset(&self) -> u64 {
        self.data_offset
    }

    #[inline(always)]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    #[inline]
    pub fn target_name(&self) -> String {
        format!("{}/{}", self.os, self.arch)
    }

    #[inline]
    pub fn mem_size(&self) -> u64 {
        self.page_num * self.page_sz
    }

    #[inline(always)]
    pub fn enabled_syscalls(&self) -> &[Syscall] {
        &self.enabled_syscalls
    }

    #[inline(always)]
    pub fn all_syscalls(&self) -> &[Syscall] {
        &self.all_syscalls
    }

    #[inline(always)]
    pub fn tys(&self) -> &[Type] {
        &self.tys
    }

    #[inline(always)]
    pub fn res_tys(&self) -> &[ResType] {
        &self.res_tys
    }

    #[inline(always)]
    pub fn res_kinds(&self) -> &[ResKind] {
        &self.res_kinds
    }

    #[inline]
    pub fn res_ty_of(&self, kind: &str) -> Option<&[TypeId]> {
        if let Some(tys) = self.res_name_mapping.get(kind) {
            Some(&tys[..])
        } else {
            None
        }
    }

    #[inline]
    pub fn res_sub_tys(&self, res_kind: &str) -> &[ResKind] {
        &self.res_sub_ty_mapping[res_kind]
    }

    #[inline]
    pub fn res_super_tys(&self, res_kind: &str) -> &[ResKind] {
        &self.res_super_ty_mapping[res_kind]
    }

    #[inline]
    pub fn res_output_syscall(&self, res_kind: &str) -> &[SyscallId] {
        &self.res_output_syscall[res_kind]
    }

    #[inline]
    pub fn res_input_syscall(&self, res_kind: &str) -> &[SyscallId] {
        &self.res_input_syscall[res_kind]
    }

    #[inline]
    pub fn syscall_output_res(&self, sid: SyscallId) -> &[ResKind] {
        &self.syscall_output_res[&sid]
    }

    #[inline]
    pub fn syscall_input_res(&self, sid: SyscallId) -> &[ResKind] {
        &self.syscall_input_res[&sid]
    }

    #[inline]
    pub fn ty_of(&self, tid: TypeId) -> &Type {
        &self.ty_id_mapping[&tid]
    }

    #[inline]
    pub fn syscall_of(&self, sid: SyscallId) -> &Syscall {
        &self.syscall_id_mapping[&sid]
    }

    #[inline]
    pub fn syscall_of_name(&self, name: &str) -> Option<&Syscall> {
        let sid = self.syscall_name_mapping.get(name)?;
        Some(&self.syscall_id_mapping[sid])
    }

    pub fn disable_syscall(&mut self, sid: SyscallId) -> Option<Syscall> {
        match self.enabled_syscalls.binary_search_by(|s| s.id().cmp(&sid)) {
            Ok(idx) => Some(self.do_remove(idx)),
            Err(_) => None,
        }
    }

    fn do_remove(&mut self, idx: usize) -> Syscall {
        let syscall = &self.enabled_syscalls[idx];
        let input_res = &self.syscall_input_res[&syscall.id()];
        let output_res = &self.syscall_output_res[&syscall.id()];
        for ir in input_res {
            let calls = self.res_input_syscall.get_mut(ir).unwrap();
            if let Ok(idx) = calls.binary_search(&syscall.id()) {
                calls.remove(idx);
            }
        }
        for or in output_res {
            let calls = self.res_output_syscall.get_mut(or).unwrap();
            if let Ok(idx) = calls.binary_search(&syscall.id()) {
                calls.remove(idx);
            }
        }
        self.enabled_syscalls.remove(idx)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TargetBuilder {
    os: Option<String>,
    arch: Option<String>,
    ptr_sz: Option<u64>,
    page_sz: Option<u64>,
    page_num: Option<u64>,
    data_offset: Option<u64>,
    le_endian: bool,
    special_ptrs: Vec<u64>,
    revision: Option<String>,

    syscalls: Vec<Syscall>,
    tys: Vec<Type>,
    res_kinds: Vec<String>,
}

impl TargetBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn os<T: Into<String>>(&mut self, os: T) -> &mut Self {
        self.os = Some(os.into());
        self
    }

    pub fn arch<T: Into<String>>(&mut self, arch: T) -> &mut Self {
        self.arch = Some(arch.into());
        self
    }

    pub fn ptr_sz(&mut self, sz: u64) -> &mut Self {
        self.ptr_sz = Some(sz);
        self
    }

    pub fn page_sz(&mut self, sz: u64) -> &mut Self {
        self.page_sz = Some(sz);
        self
    }

    pub fn page_num(&mut self, num: u64) -> &mut Self {
        self.page_num = Some(num);
        self
    }

    pub fn le_endian(&mut self, le: bool) -> &mut Self {
        self.le_endian = le;
        self
    }

    pub fn revision<T: Into<String>>(&mut self, revision: T) -> &mut Self {
        self.revision = Some(revision.into());
        self
    }

    pub fn special_ptrs(&mut self, ptrs: Vec<u64>) -> &mut Self {
        self.special_ptrs = ptrs;
        self
    }

    pub fn data_offset(&mut self, data_offset: u64) -> &mut Self {
        self.data_offset = Some(data_offset);
        self
    }

    pub fn syscalls(&mut self, syscalls: Vec<Syscall>) -> &mut Self {
        self.syscalls = syscalls;
        self
    }

    pub fn tys(&mut self, tys: Vec<Type>) -> &mut Self {
        self.tys = tys;
        self
    }

    pub fn res_kinds(&mut self, res_kinds: Vec<String>) -> &mut Self {
        self.res_kinds = res_kinds;
        self
    }

    pub fn build(self) -> Target {
        // all syscalls
        let syscalls = self.syscalls;
        let syscall_id_mapping = syscalls
            .iter()
            .map(|s| (s.id(), s.clone()))
            .collect::<HashMap<_, _>>();
        let syscall_name_mapping = syscalls
            .iter()
            .map(|s| (s.name().to_string().into_boxed_str(), s.id()))
            .collect::<HashMap<_, _>>();
        // all tys
        let tys = self.tys;
        let ty_id_mapping = tys
            .iter()
            .map(|s| (s.id(), s.clone()))
            .collect::<HashMap<_, _>>();
        // all res tys
        let mut res_tys = tys
            .iter()
            .filter_map(|ty| ty.as_res().cloned())
            .collect::<Vec<_>>();
        res_tys.sort_unstable();
        res_tys.dedup();
        // name to res type id
        let mut res_name_mapping: HashMap<ResKind, Vec<TypeId>> = HashMap::default();
        for res_ty in res_tys.iter() {
            let entry = res_name_mapping
                .entry(res_ty.res_name().to_string().into_boxed_str())
                .or_default();
            entry.push(res_ty.id());
        }
        // super type of each resource
        let mut res_super_ty_mapping: HashMap<String, HashSet<String>> = self
            .res_kinds
            .iter()
            .map(|kind| (kind.to_string(), HashSet::default()))
            .collect();
        // sub type of each resource
        let mut res_sub_ty_mapping: HashMap<String, HashSet<String>> = self
            .res_kinds
            .iter()
            .map(|kind| (kind.to_string(), HashSet::default()))
            .collect();
        for res_a in res_tys.iter() {
            let kind = res_a.res_name();
            let entry = res_super_ty_mapping.entry(kind.to_string()).or_default();
            res_a.kinds().iter().for_each(|kind| {
                entry.insert(kind.to_string());
            });
            for res_b in res_a.kinds() {
                let entry = res_sub_ty_mapping.entry(res_b.to_string()).or_default();
                entry.insert(kind.to_string());
            }
        }
        let res_super_ty_mapping = res_super_ty_mapping
            .into_iter()
            .map(|(kind, super_kinds)| {
                let mut super_kinds = super_kinds
                    .into_iter()
                    .map(|v| v.into_boxed_str())
                    .collect::<Vec<_>>();
                super_kinds.sort_unstable();
                (kind.into_boxed_str(), super_kinds)
            })
            .collect();
        let res_sub_ty_mapping: HashMap<ResKind, Vec<ResKind>> = res_sub_ty_mapping
            .into_iter()
            .map(|(kind, sub_kinds)| {
                let mut sub_kinds = sub_kinds
                    .into_iter()
                    .map(|v| v.into_boxed_str())
                    .collect::<Vec<_>>();
                sub_kinds.sort_unstable();
                (kind.into_boxed_str(), sub_kinds)
            })
            .collect();
        // collect input/output syscalls for resources
        let mut syscall_input_res = HashMap::default();
        let mut syscall_output_res = HashMap::default();
        let mut res_input_syscall: HashMap<ResKind, HashSet<SyscallId>> = self
            .res_kinds
            .iter()
            .map(|kind| (kind.to_string().into_boxed_str(), HashSet::default()))
            .collect();
        let mut res_output_syscall: HashMap<ResKind, HashSet<SyscallId>> = self
            .res_kinds
            .iter()
            .map(|kind| (kind.to_string().into_boxed_str(), HashSet::default()))
            .collect();
        for syscall in syscalls.iter() {
            let (input_res, output_res) = analyze_res_usage(syscall, &ty_id_mapping);
            for ir in &input_res {
                res_input_syscall
                    .get_mut(&ir[..])
                    .unwrap()
                    .insert(syscall.id());
            }
            for or in &output_res {
                res_output_syscall
                    .get_mut(&or[..])
                    .unwrap()
                    .insert(syscall.id());
            }
            syscall_input_res.insert(syscall.id(), input_res);
            syscall_output_res.insert(syscall.id(), output_res);
        }
        // add sub type's output syscall to a resource, if it does not contain any output call.
        let res_no_out_calls = res_output_syscall
            .iter()
            .filter(|(_, syscalls)| syscalls.is_empty())
            .map(|(kind, _)| kind.to_string())
            .collect::<Vec<_>>();
        for res in res_no_out_calls {
            let sub_tys = &res_sub_ty_mapping[&res[..]];
            for sub_ty in sub_tys {
                let sids = res_output_syscall.get(&sub_ty[..]).unwrap().clone();
                let out_syscalls = res_output_syscall.get_mut(&res[..]).unwrap();
                for sid in sids {
                    out_syscalls.insert(sid);
                }
            }
        }
        let res_input_syscall = res_input_syscall
            .into_iter()
            .map(|(kind, syscalls)| {
                let mut syscalls = syscalls.into_iter().collect::<Vec<_>>();
                syscalls.sort_unstable();
                (kind, syscalls)
            })
            .collect();
        let res_output_syscall = res_output_syscall
            .into_iter()
            .map(|(kind, syscalls)| {
                let mut syscalls = syscalls.into_iter().collect::<Vec<_>>();
                syscalls.sort_unstable();
                (kind, syscalls)
            })
            .collect();

        Target {
            os: self.os.unwrap().into_boxed_str(),
            arch: self.arch.unwrap().into_boxed_str(),
            ptr_sz: self.ptr_sz.unwrap(),
            page_sz: self.page_sz.unwrap(),
            page_num: self.page_num.unwrap(),
            le_endian: self.le_endian,
            special_ptrs: self.special_ptrs.into_boxed_slice(),
            data_offset: self.data_offset.unwrap_or_default(),

            revision: self.revision.unwrap().into_boxed_str(),
            all_syscalls: syscalls.clone(),
            enabled_syscalls: syscalls,
            tys,
            syscall_id_mapping,
            ty_id_mapping,
            syscall_name_mapping,
            res_tys,
            res_name_mapping,
            res_kinds: self
                .res_kinds
                .into_iter()
                .map(|v| v.into_boxed_str())
                .collect(),
            res_super_ty_mapping,
            res_sub_ty_mapping,

            syscall_input_res,
            syscall_output_res,
            res_input_syscall,
            res_output_syscall,
        }
    }
}

pub fn analyze_res_usage(
    syscall: &Syscall,
    tys: &HashMap<TypeId, Type>,
) -> (Vec<ResKind>, Vec<ResKind>) {
    let mut ctx = AnalyzeContext {
        tys,
        input_res: HashSet::default(),
        output_res: HashSet::default(),
        visited: HashSet::default(),
    };

    for field in syscall.params() {
        analyze_ty(&mut ctx, field.ty(), Dir::In);
    }

    if let Some(ty) = syscall.ret().as_ref() {
        analyze_ty(&mut ctx, ty, Dir::Out);
    }

    let mut input_res = ctx.input_res.into_iter().collect::<Vec<_>>();
    let mut output_res = ctx.output_res.into_iter().collect::<Vec<_>>();
    input_res.sort_unstable();
    output_res.sort_unstable();
    (input_res, output_res)
}

struct AnalyzeContext<'a> {
    tys: &'a HashMap<TypeId, Type>,
    input_res: HashSet<ResKind>,
    output_res: HashSet<ResKind>,
    visited: HashSet<TypeId>,
}

fn analyze_ty(ctx: &mut AnalyzeContext, ty: &Type, dir: Dir) {
    match ty.kind() {
        TypeKind::Res => {
            if dir != Dir::In {
                ctx.output_res
                    .insert(ty.checked_as_res().res_name().to_string().into_boxed_str());
            } else if !ty.optional() {
                ctx.input_res
                    .insert(ty.checked_as_res().res_name().to_string().into_boxed_str());
            }
        }
        TypeKind::Ptr => {
            let ty = ty.checked_as_ptr();
            let elem_ty = &ctx.tys[&ty.elem()];
            let elem_dir = ty.dir();
            analyze_ty(ctx, elem_ty, elem_dir);
        }
        TypeKind::Array => {
            let ty = ty.checked_as_array();
            let elem_ty = ty.elem();
            analyze_ty(ctx, elem_ty, dir);
        }
        TypeKind::Struct => {
            let ty = ty.checked_as_struct();
            if ctx.visited.insert(ty.id()) {
                for f in ty.fields() {
                    analyze_ty(ctx, f.ty(), f.dir().unwrap_or(dir));
                }
            }
        }
        TypeKind::Union => {
            let ty = ty.checked_as_union();
            if ctx.visited.insert(ty.id()) {
                for f in ty.fields() {
                    analyze_ty(ctx, f.ty(), f.dir().unwrap_or(dir));
                }
            }
        }
        _ => (),
    }
}
