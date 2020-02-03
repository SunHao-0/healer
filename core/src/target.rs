use std::collections::HashMap;

use fots::types::{FnId, FnInfo, Group, GroupId, Items, TypeId, TypeInfo, Field};
use std::ptr::NonNull;

pub struct Target {
    pub types: HashMap<TypeId, TypeInfo>,
    pub groups: HashMap<GroupId, Group>,
    pub fns: HashMap<FnId, NonNull<FnInfo>>,
}

impl Target {
    pub fn new(items: Items) -> Self {
        let mut types = items
            .types
            .into_iter()
            .map(|t| (t.tid, t.info))
            .collect::<HashMap<_, _>>();
        types.shrink_to_fit();
        let mut groups = items
            .groups
            .into_iter()
            .map(|g| (g.id, g))
            .collect::<HashMap<_, _>>();
        groups.shrink_to_fit();
        let mut fns: HashMap<FnId, NonNull<FnInfo>> = groups
            .values()
            .flat_map(|g| g.iter_fn().map(|f| (f.id, NonNull::from(f))))
            .collect();
        fns.shrink_to_fit();

        Target { groups, types, fns }
    }

    pub fn type_of(&self, tid: TypeId) -> &TypeInfo {
        &self.types.get(&tid).unwrap()
    }

    pub fn fn_of(&self, fid: FnId) -> &FnInfo {
        unsafe { self.fns[&fid].as_ref() }
    }

    pub fn iter_group(&self) -> impl Iterator<Item=&Group> + '_ {
        self.groups.values()
    }

    pub fn is_res(&self, tid: TypeId) -> bool {
        match self.type_of(tid) {
            TypeInfo::Alias { tid, .. } => self.is_res(*tid),
            TypeInfo::Res { .. } => true,
            _ => false,
        }
    }

    pub fn is_str(&self, tid: TypeId) -> bool {
        match self.type_of(tid) {
            TypeInfo::Alias { tid, .. } => self.is_res(*tid),
            TypeInfo::Str { .. } => true,
            _ => false,
        }
    }

    pub fn is_slice(&self, tid: TypeId) -> bool {
        match self.type_of(tid) {
            TypeInfo::Alias { tid, .. } => self.is_res(*tid),
            TypeInfo::Slice { .. } => true,
            _ => false,
        }
    }

    pub fn len_info_of(&self, tid: TypeId) -> Option<&str> {
        match self.type_of(tid) {
            TypeInfo::Alias { tid, .. } => self.len_info_of(*tid),
            TypeInfo::Len { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn struct_info_of(&self, tid: TypeId) -> Option<(&str, &[Field])> {
        match self.type_of(tid) {
            TypeInfo::Struct { fields, ident } => Some((ident, fields)),
            TypeInfo::Alias { tid, .. } => self.struct_info_of(*tid),
            _ => None,
        }
    }

    pub fn get_len_path_unchecked(&self, tid: TypeId) -> &str {
        self.len_info_of(tid).unwrap()
    }
}
