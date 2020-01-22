use std::collections::HashMap;

use fots::types::{Group, GroupId, Items, TypeId, TypeInfo};

pub struct Target {
    pub types: HashMap<TypeId, TypeInfo>,
    pub groups: HashMap<GroupId, Group>,
    pub gids: Vec<GroupId>,
}

impl Target {
    pub fn new(items: Items) -> Self {
        let mut types = items
            .types
            .into_iter()
            .map(|t| (t.tid, t.info))
            .collect::<HashMap<_, _>>();
        let mut groups = items
            .groups
            .into_iter()
            .map(|g| (g.id, g))
            .collect::<HashMap<_, _>>();
        let mut gids = groups.iter().map(|(&id, _)| id).collect::<Vec<_>>();
        types.shrink_to_fit();
        groups.shrink_to_fit();
        gids.shrink_to_fit();

        let target = Target {
            groups,
            types,
            gids,
        };
        target
    }

    pub fn type_of(&self, tid: TypeId) -> &TypeInfo {
        &self.types.get(&tid).unwrap()
    }

    pub fn iter_group(&self) -> impl Iterator<Item = &Group> + '_ {
        self.groups.values()
    }

    pub fn is_res(&self, tid: TypeId) -> bool {
        match self.type_of(tid) {
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
