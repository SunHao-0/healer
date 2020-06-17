use crate::analyze::RTable;
use crate::gen::{gen_seq, Config};
use crate::minimize::remove;
use crate::prog::Prog;
use crate::target::Target;
use rand::prelude::*;
use std::collections::{HashSet, HashMap};
use fots::types::GroupId;

#[allow(clippy::type_complexity)]
const MUTATE_METHOD: [fn(&Prog, &Target, &RTable, &HashSet<Prog>, &Config) -> Prog; 3] =
    [seq_reuse, merge_seq, remove_call];

pub fn mutate( corpus: &HashSet<Prog>, t: &Target, rt: &HashMap<GroupId, RTable>, conf: &Config) -> Prog {
    let mut rng = thread_rng();
    let p = corpus.iter().choose(&mut rng).unwrap();
    let rt = &rt[&p.gid];
    let method = MUTATE_METHOD.choose(&mut rng).unwrap();
    method(p, t, rt, corpus, conf)
}

fn seq_reuse(p: &Prog, t: &Target, _rt: &RTable, _corpus: &HashSet<Prog>, conf: &Config) -> Prog {
    let seq = extract_seq(p, t);
    gen_seq(&seq, p.gid, t, conf)
}

fn extract_seq(p: &Prog, t: &Target) -> Vec<usize> {
    let g = &t.groups[&p.gid];
    let mut seq = Vec::new();
    for c in &p.calls {
        seq.push(g.fns.iter().position(|f| f.id == c.fid).unwrap())
    }
    seq
}

fn merge_seq(p0: &Prog, t: &Target, _rt: &RTable, corpus: &HashSet<Prog>, conf: &Config) -> Prog {
    let mut rng = thread_rng();
    let merge_point = rng.gen_range(0, p0.len());
    let mut s0 = extract_seq(p0, t);
    let p1 = corpus.iter().filter(|p1| p1.gid == p0.gid).choose(&mut rng);
    if let Some(p1) = p1 {
        let s1 = extract_seq(p1, t);
        let left = s0.split_off(merge_point + 1);
        s0.extend(s1);
        s0.extend(left);
    }
    gen_seq(&s0, p0.gid, t, conf)
}

// fn insert_call(p: &Prog, t: &Target, rt: &RTable, corpus: &[Prog], conf: &Config) -> Prog {
//     let seq = ex
//     todo!()
// }

fn remove_call(p: &Prog, t: &Target, rt: &RTable, corpus: &HashSet<Prog>, conf: &Config) -> Prog {
    let mut rng = thread_rng();
    if p.len() <= 3 {
        let (_, methods) = MUTATE_METHOD.split_last().unwrap();
        let method = methods.choose(&mut rng).unwrap();
        return method(&p, t, rt, corpus, conf);
    }

    let mut p = p.clone();
    let mut retry = 0;
    loop {
        let c = rng.gen_range(0, p.len());
        if remove(&mut p, c) {
            return p;
        } else if retry != p.len() {
            retry += 1;
            continue;
        } else {
            let (_, methods) = MUTATE_METHOD.split_last().unwrap();
            let method = methods.choose(&mut rng).unwrap();
            return method(&p, t, rt, corpus, conf);
        }
    }
}
