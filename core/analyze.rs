//! Analyze
//!
//! Analyze relation between interface. The relation between
//! interface can only be 1/0.
use crate::prog::Prog;
use crate::target::Target;
use fots::types::{FnInfo, Group, GroupId, PtrDir, TypeId, TypeInfo};
use ndarray::{Array2, Axis};
use std::collections::HashMap;
use std::fmt::{Display, Error, Formatter};
use std::ops::{Deref, DerefMut};

/// Relation between interface
#[derive(Debug, Clone, PartialOrd, PartialEq, Eq, Ord)]
pub enum Relation {
    None,
    Some,
}

impl Display for Relation {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let fm = match self {
            Relation::None => "0",
            Relation::Some => "1",
        };
        write!(f, "{}", fm)
    }
}

impl Default for Relation {
    fn default() -> Self {
        Relation::None
    }
}

/// Table of relation
#[derive(Debug, Clone)]
pub struct RTable(Array2<Relation>);

impl RTable {
    /// Crate new relation table for n interfaces, use default value for relation
    pub fn new(n: usize) -> Self {
        RTable(Array2::default((n, n)))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len_of(Axis(0))
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
    if f.has_params() {
        for p in f.iter_param() {
            let mut id = p.tid;
            let mut in_ = true;
            if let TypeInfo::Ptr { tid, dir, depth } = t.type_of(id) {
                assert!(*depth == 1, "Multi-level pointer not supported");
                id = *tid;
                in_ = *dir == PtrDir::In;
            }
            if t.is_res(id) {
                record_use(uses, id, index, in_);
            }
        }
    }

    if let Some(tid) = f.r_tid {
        if t.is_res(tid) {
            record_use(uses, tid, index, false);
        }
    }
}

fn record_use(uses: &mut HashMap<TypeId, Use>, res: TypeId, fn_index: usize, in_: bool) {
    let u = uses.entry(res).or_insert_with(Default::default);
    if in_ {
        u.consumer.push(fn_index)
    } else {
        u.producer.push(fn_index)
    }
}

/// Analyze call seq of prog, update RTable
///
/// Analysis is based on the order of target in a prog.
/// If A is before B in a prog, then B has impact on A.
/// Thr prog must be minimized befor being used.
pub fn prog_analyze(g: &Group, r: &mut RTable, p: &Prog) {
    assert!(!p.is_empty());
    let mut id_index = Vec::new();

    for c in &p.calls {
        if let Some(index) = g.index_by_id(c.fid) {
            id_index.push(index);
        } else {
            panic!("fn{} out of group{}", c.fid, g.id);
        }
    }

    for i in (0..id_index.len()).rev() {
        if i != 0 {
            r[(id_index[i], id_index[i - 1])] = Relation::Some;
        }
    }
}
