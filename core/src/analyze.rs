use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::ops::{Deref, DerefMut};

use ndarray::Array2;

use fots::types::{FnInfo, Group, GroupId, PtrDir, TypeId, TypeInfo};

use crate::prog::Prog;
use crate::target::Target;

/// Relation between interface
#[derive(Debug, Clone)]
pub enum Relation {
    None,
    Some,
    Unknown,
}

impl Display for Relation {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let fm = match self {
            Relation::None => "0",
            Relation::Some => "1",
            Relation::Unknown => "2",
        };
        write!(f, "{}", fm)
    }
}

impl Default for Relation {
    fn default() -> Self {
        Relation::Unknown
    }
}

/// Table of relation
#[derive(Debug, Clone)]
pub struct RTable(Array2<Relation>);

impl RTable {
    /// Crate new relation table for n interfaces, use default value for relation
    pub fn new(n: usize) -> Self {
        let mut rt = RTable(Array2::default((n, n)));
        for i in 0..n {
            rt[(i, i)] = Relation::None;
        }
        rt
    }
}

impl Deref for RTable {
    type Target = Array2<Relation>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for RTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{}", self.0)
    }
}

const FUNC_ATTR_IMPACT: &str = "impact";

/// Analyze params,attrs of interface, return static relation table
///
/// Analysis is based on input/output and attrs of interface.
/// For example, if interface A takes input from interface B, then
/// B has impact on A. The *impact* attr also imply relation.
pub fn static_analyze(target: &Target) -> HashMap<GroupId, RTable> {
    let mut rs = HashMap::new();
    for g in target.iter_group() {
        let mut r = RTable::new(g.fn_num());
        res_analyze(g, &mut r, &target);
        attr_analyze(g, &mut r);
        rs.insert(g.id, r);
    }
    rs
}

fn res_analyze(g: &Group, r: &mut RTable, t: &Target) {
    let mut uses = HashMap::new();
    g.iter_fn()
        .enumerate()
        .for_each(|(i, f)| res_use(i, f, t, &mut uses));
    for u in uses.values() {
        for &p in &u.producer {
            for &c in &u.consumer {
                if p != c {
                    r[(c, p)] = Relation::Some;
                }
            }
        }
    }
}

fn attr_analyze(g: &Group, r: &mut RTable) {
    for (i, f) in g.iter_fn().enumerate() {
        if let Some(attr) = f.get_attr(FUNC_ATTR_IMPACT) {
            if attr.has_vals() {
                for val in attr.iter_val() {
                    if let Some(j) = g.index_by_name(val) {
                        r[(j, i)] = Relation::Some;
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct Use {
    consumer: Vec<usize>,
    producer: Vec<usize>,
}

fn res_use(index: usize, f: &FnInfo, t: &Target, uses: &mut HashMap<TypeId, Use>) {
    for p in f.iter_param() {
        let mut id = p.tid;
        let mut in_ = true;
        if let TypeInfo::Ptr { tid, dir, depth } = t.type_info_of(id) {
            assert!(*depth == 1, "Multi-level pointer not supported");
            id = *tid;
            in_ = *dir == PtrDir::In;
        }
        if t.is_res(id) {
            record_use(uses, id, index, in_);
        }
    }

    if let Some(tid) = f.r_tid {
        if t.is_res(tid) {
            record_use(uses, tid, index, false);
        }
    }
}

fn record_use(uses: &mut HashMap<TypeId, Use>, res: TypeId, fn_index: usize, in_: bool) {
    let u = uses.entry(res).or_insert(Default::default());
    if in_ {
        u.consumer.push(fn_index)
    } else {
        u.producer.push(fn_index)
    }
}

/// Analyze call seq of prog, update RTable
///
/// Analysis is based on the order of calls in a prog.
/// If A is before B in a prog, then B has impact on A.
/// Thr prog must be minimized befor being used.
pub fn prog_analyze(g: &Group, r: &mut RTable, p: &Prog) {
    assert!(p.len() != 0);
    let mut id_index = Vec::new();

    for c in &p.calls {
        if let Some(index) = g.index_by_id(c.fid) {
            id_index.push(index);
        } else {
            panic!("fn{} out of group{}", c.fid, g.id);
        }
    }

    for i in (0..id_index.len()).rev() {
        for j in 0..i {
            r[(id_index[i], id_index[j])] = Relation::Some;
        }
    }
}



