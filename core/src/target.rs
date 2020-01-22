use std::collections::HashMap;

use fots::types::{Group, GroupId, Items, TypeId, TypeInfo};

pub struct Target {
    pub types: HashMap<TypeId, TypeInfo>,
    pub groups: HashMap<GroupId, Group>,
    pub gids: Vec<GroupId>,
}

impl Target {
    pub fn new(items: Items) -> Self {
        let types = items
            .types
            .into_iter()
            .map(|t| (t.tid, t.info))
            .collect::<HashMap<_, _>>();
        let groups = items
            .groups
            .into_iter()
            .map(|g| (g.id, g))
            .collect::<HashMap<_, _>>();
        let gids = groups.iter().map(|(&id, _)| id).collect();
        let mut target = Target {
            groups,
            types,
            gids,
        };
        target
    }

    pub fn type_info_of(&self, tid: TypeId) -> &TypeInfo {
        &self.types.get(&tid).unwrap()
    }

    pub fn iter_group(&self) -> impl Iterator<Item=&Group> + '_ {
        self.groups.values()
    }

    pub fn is_res(&self, tid: TypeId) -> bool {
        match self.type_info_of(tid) {
            TypeInfo::Alias { tid, .. } => self.is_res(*tid),
            TypeInfo::Res { .. } => true,
            _ => false,
        }
    }
}

pub enum ResDir {
    In,
    Out,
}
