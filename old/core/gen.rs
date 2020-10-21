//! Generation
//!
//! Program generation is based on relation table.
//! Consider interface A, if A's execution only depends on its
//! input which means it doesn't has read dep of external/global
//! state in its condition node inside of its code, then we can
//! traverse every execution path of A by numbers of random input.
//! However if A has read dep on its condition node, then its ex-
//! ecution flow may also depends on extern/global state too, wh-
//! ich means it maynot be possible to traverse its execution flow
//! samply by number of random input. In this case, we need add
//! some other interfaces that modify that external/global state
//! which means generating sequence of target not single call.
use std::collections::HashMap;
use std::path::PathBuf;

use ndarray::Axis;
use rand::distributions::Alphanumeric;
use rand::prelude::*;
use rand::{random, thread_rng, Rng};

use fots::types::{
    Field, Flag, FnInfo, GroupId, NumInfo, NumLimit, PtrDir, StrType, TypeId, TypeInfo,
};

use crate::analyze::{RTable, Relation};
use crate::prog::{Arg, ArgIndex, ArgPos, Call, Prog};
use crate::target::Target;
use crate::value::{NumValue, Value};

#[derive(Clone)]
pub struct Config {
    pub prog_max_len: usize,
    pub prog_min_len: usize,
    pub str_min_len: usize,
    pub str_max_len: usize,
    pub path_max_depth: usize,
    pub sp_delta: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            prog_max_len: 16,
            prog_min_len: 1,
            str_min_len: 0,
            str_max_len: 32,
            path_max_depth: 4,
            sp_delta: 0.4,
        }
    }
}

pub fn gen<S: std::hash::BuildHasher>(
    t: &Target,
    rs: &HashMap<GroupId, RTable, S>,
    conf: &Config,
) -> Prog {
    assert!(!rs.is_empty());
    assert_eq!(t.groups.len(), rs.len());

    let mut rng = thread_rng();
    // choose group
    let gid = rs.keys().choose(&mut rng).unwrap();
    gen_prog(*gid, &rs[gid], t, conf)
}

pub fn gen_prog(gid: GroupId, r: &RTable, t: &Target, conf: &Config) -> Prog {
    // choose sequence
    let seq = choose_seq(r, conf);
    assert!(!seq.is_empty());

    gen_seq(&seq, gid, t, conf)
}

pub fn gen_seq(seq: &[usize], gid: GroupId, t: &Target, conf: &Config) -> Prog {
    let g = &t.groups[&gid];
    assert!(!g.fns.is_empty());

    // gen value
    let mut s = State::new(Prog::new(g.id), conf);
    for &i in seq.iter() {
        gen_call(t, &g.fns[i], &mut s);
    }
    adjust_size_param(&mut s.prog, t);
    s.prog
}

fn adjust_size_param(p: &mut Prog, t: &Target) {
    for c in &mut p.calls.iter_mut() {
        let f = t.fn_of(c.fid);
        if f.has_params() {
            for (i, p) in f.iter_param().enumerate() {
                if let Some(path) = t.len_info_of(p.tid) {
                    if let Some(j) = f.iter_param().position(|p| p.ident == path) {
                        if let Some(l) = c.args[j].val.len() {
                            c.args[i].val = Value::Num(NumValue::Unsigned(l as u64));
                        }
                    }
                } else {
                    adjust_size(p.tid, &mut c.args[i].val, t);
                }
            }
        }
    }
}

fn adjust_size(tid: TypeId, v: &mut Value, t: &Target) {
    match t.type_of(tid) {
        TypeInfo::Ptr { tid, .. } => {
            if v != &Value::None {
                adjust_size(*tid, v, t);
            }
        }
        TypeInfo::Slice { tid, .. } => {
            if let Value::Group(vals) = v {
                for v in vals.iter_mut() {
                    adjust_size(*tid, v, t);
                }
            }
        }
        TypeInfo::Struct { fields, .. } => {
            let vals = if let Value::Group(vals) = v {
                vals
            } else {
                panic!()
            };
            asign_struct(fields, vals, t);
        }
        TypeInfo::Alias { tid, .. } => adjust_size(*tid, v, t),
        _ => (),
    }
}

fn asign_struct(fields: &[Field], vals: &mut [Value], t: &Target) {
    for (i, f) in fields.iter().enumerate() {
        if let Some(p) = t.len_info_of(f.tid) {
            asign_len_val(i, p, fields, vals, t)
        } else if let Some((_, fields)) = t.struct_info_of(f.tid) {
            let vals = if let Value::Group(val) = &mut vals[i] {
                val
            } else {
                panic!()
            };
            asign_struct(fields, vals, t)
        }
    }
}

fn asign_len_val(index: usize, path: &str, fields: &[Field], vals: &mut [Value], t: &Target) {
    let mut sub_paths = path.split('.');
    let mut p = sub_paths.next().unwrap();

    let mut crt_field = fields;
    let mut crt_vals = &vals[..];
    loop {
        if let Some(i) = crt_field.iter().position(|f| f.ident == p) {
            if let Some(n_p) = sub_paths.next() {
                if let Some((_, n_fields)) = t.struct_info_of(crt_field[i].tid) {
                    crt_field = n_fields;
                } else {
                    panic!();
                }
                crt_vals = if let Value::Group(val) = &crt_vals[i] {
                    val
                } else {
                    panic!()
                };
                p = n_p;
            } else {
                if let Some(l) = crt_vals[i].len() {
                    vals[index] = Value::Num(NumValue::Unsigned(l as u64));
                }
                break;
            }
        } else {
            panic!()
        }
    }
}

struct State<'a> {
    res: HashMap<TypeId, Vec<ArgIndex>>,
    strs: HashMap<StrType, Vec<String>>,
    prog: Prog,
    conf: &'a Config,
}

impl<'a> State<'a> {
    pub fn new(prog: Prog, conf: &'a Config) -> Self {
        Self {
            res: HashMap::new(),
            strs: hashmap! {StrType::FileName => Vec::new()},
            prog,
            conf,
        }
    }

    pub fn record_res(&mut self, tid: TypeId, is_ret: bool) {
        let cid = self.prog.len() - 1;

        let idx = self.res.entry(tid).or_insert_with(Default::default);
        if is_ret {
            idx.push((cid, ArgPos::Ret))
        } else {
            let arg_pos = self.prog.calls[cid].args.len() - 1;
            idx.push((cid, ArgPos::Arg(arg_pos)))
        }
    }

    pub fn record_str(&mut self, t: StrType, val: &str) {
        let vals = self.strs.entry(t).or_insert_with(Default::default);
        vals.push(val.into())
    }

    pub fn try_reuse_res(&self, tid: TypeId) -> Option<Value> {
        let mut rng = thread_rng();
        if let Some(res) = self.res.get(&tid) {
            if !res.is_empty() {
                let r = res.choose(&mut rng).unwrap();
                return Some(Value::Ref(r.clone()));
            }
        }
        None
    }

    pub fn try_reuse_str(&self, str_type: StrType) -> Option<Value> {
        let mut rng = thread_rng();
        if let Some(strs) = self.strs.get(&str_type) {
            if !strs.is_empty() && rng.gen() {
                let s = strs.choose(&mut rng).unwrap();
                return Some(Value::Str(s.clone()));
            }
        }
        None
    }

    // add call
    pub fn add_call(&mut self, call: Call) -> &mut Call {
        self.prog.add_call(call)
    }

    // Add arg for last call
    pub fn add_arg(&mut self, arg: Arg) -> &mut Arg {
        let i = self.prog.len() - 1;
        self.prog.calls[i].add_arg(arg)
    }

    pub fn add_ret(&mut self, arg: Arg) -> &mut Arg {
        let i = self.prog.len() - 1;
        self.prog.calls[i].ret = Some(arg);
        self.prog.calls[i].ret.as_mut().unwrap()
    }

    pub fn update_val(&mut self, val: Value) {
        let c = self.prog.calls.last_mut().unwrap();
        let arg_index = c.args.len() - 1;
        c.args[arg_index].val = val;
    }
}

fn gen_call(t: &Target, f: &FnInfo, s: &mut State) {
    s.add_call(Call::new(f.id));

    if f.has_params() {
        for p in f.iter_param() {
            s.add_arg(Arg::new(p.tid));
            let val = gen_value(p.tid, t, s);
            s.update_val(val);
        }
    }

    if let Some(tid) = f.r_tid {
        if t.is_res(tid) {
            s.add_ret(Arg::new(tid));
            s.record_res(tid, true);
        }
    }
}

/// generate value for any type
fn gen_value(tid: TypeId, t: &Target, s: &mut State) -> Value {
    match t.type_of(tid) {
        TypeInfo::Num(num_info) => gen_num(num_info),
        TypeInfo::Ptr { dir, tid, depth } => {
            assert_eq!(*depth, 1, "Multi-level pointer not supported");
            gen_ptr(*dir, *tid, t, s)
        }
        // TODO  what if tid is type of res
        TypeInfo::Slice { tid, l, h } => gen_slice(*tid, *l, *h, t, s),
        TypeInfo::Str { str_type, vals } => gen_str(str_type, vals, s),
        TypeInfo::Struct { fields, .. } => gen_struct(&fields[..], t, s),
        TypeInfo::Union { fields, .. } => gen_union(&fields[..], t, s),
        TypeInfo::Flag { flags, .. } => gen_flag(&flags[..]),

        TypeInfo::Alias { tid: under_id, .. } => gen_alias(tid, *under_id, t, s),
        TypeInfo::Res { tid: under_tid } => gen_res(tid, *under_tid, t, s),
        TypeInfo::Len { .. } => Value::Num(NumValue::Unsigned(0)),
    }
}

fn gen_alias(tid: TypeId, under_id: TypeId, t: &Target, s: &mut State) -> Value {
    if t.is_res(tid) {
        gen_res(tid, under_id, t, s)
    } else {
        gen_value(under_id, t, s)
    }
}

fn gen_res(res_tid: TypeId, tid: TypeId, t: &Target, s: &mut State) -> Value {
    if let Some(res) = s.try_reuse_res(res_tid) {
        res
    } else {
        gen_value(tid, t, s)
    }
}

fn gen_ptr(dir: PtrDir, tid: TypeId, t: &Target, s: &mut State) -> Value {
    if dir != PtrDir::In {
        if t.is_res(tid) {
            s.record_res(tid, false);
        }
        return Value::default_val(tid, t);
    }

    if thread_rng().gen::<f64>() >= 0.001 {
        gen_value(tid, t, s)
    } else {
        Value::None
    }
}

fn gen_flag(flags: &[Flag]) -> Value {
    assert!(!flags.is_empty());

    let mut rng = thread_rng();

    if rng.gen::<f64>() < 0.005 {
        Value::Num(NumValue::Signed(rng.gen::<u8>() as i64))
    } else {
        let flag = flags.iter().choose(&mut rng).unwrap();
        let mut val = flag.val;

        loop {
            if rng.gen() {
                let flag = flags.iter().choose(&mut rng).unwrap();
                val &= flag.val;
            } else {
                break;
            }
        }
        Value::Num(NumValue::Signed(val))
    }
}

fn gen_union(fields: &[Field], t: &Target, s: &mut State) -> Value {
    assert!(!fields.is_empty());

    let i = thread_rng().gen_range(0, fields.len());
    let field = &fields[i];

    Value::Opt {
        choice: i,
        val: Box::new(gen_value(field.tid, t, s)),
    }
}

fn gen_struct(fields: &[Field], t: &Target, s: &mut State) -> Value {
    let mut vals = Vec::new();
    for field in fields.iter() {
        vals.push(gen_value(field.tid, t, s));
    }
    Value::Group(vals)
}

fn gen_str(str_type: &StrType, vals: &Option<Vec<String>>, s: &mut State) -> Value {
    let mut rng = thread_rng();
    if let Some(vals) = vals {
        if !vals.is_empty() {
            return Value::Str(vals.choose(&mut rng).unwrap().clone());
        }
    }
    if let Some(s) = s.try_reuse_str(str_type.clone()) {
        return s;
    }

    let len = rng.gen_range(s.conf.str_min_len, s.conf.str_max_len);
    match str_type {
        StrType::Str => {
            //            let val = rng
            //                .sample_iter::<char, Standard>(Standard)
            //                .take(len)
            //                .collect::<String>();
            let val = rng.sample_iter(Alphanumeric).take(len).collect::<String>();
            s.record_str(StrType::Str, &val);
            Value::Str(val)
        }
        StrType::CStr => {
            let val = rng.sample_iter(Alphanumeric).take(len).collect::<String>();
            s.record_str(StrType::CStr, &val);
            Value::Str(val)
        }
        StrType::FileName => {
            let mut path = PathBuf::from(".");
            let mut depth = 0;
            loop {
                let sub_path = rng.sample_iter(Alphanumeric).take(len).collect::<String>();
                path.push(sub_path);
                depth += 1;
                if depth < s.conf.path_max_depth && rng.gen::<f64>() > 0.4 {
                    continue;
                } else if let Ok(p) = path.into_os_string().into_string() {
                    s.record_str(StrType::FileName, &p);
                    return Value::Str(p);
                } else {
                    path = PathBuf::from(".");
                    depth = 0;
                }
            }
        }
    }
}

fn gen_slice(tid: TypeId, l: isize, h: isize, t: &Target, s: &mut State) -> Value {
    let len: usize = gen_slice_len(l, h);
    let mut vals = Vec::new();

    for _ in 0..len {
        vals.push(gen_value(tid, t, s));
    }
    Value::Group(vals)
}

pub(crate) fn gen_slice_len(l: isize, h: isize) -> usize {
    match (l, h) {
        (-1, -1) => thread_rng().gen_range(1, 8),
        (l, -1) => l as usize,
        (l, h) => thread_rng().gen_range(l as usize, h as usize),
    }
}

fn gen_num(type_info: &NumInfo) -> Value {
    let mut rng = thread_rng();

    match type_info {
        NumInfo::I8(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Signed(*vals.choose(&mut rng).unwrap() as i64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Signed(rng.gen_range(r.start, r.end) as i64))
            }
            NumLimit::None => Value::Num(NumValue::Signed(rng.gen::<i8>() as i64)),
        },
        NumInfo::I16(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Signed(*vals.choose(&mut rng).unwrap() as i64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Signed(rng.gen_range(r.start, r.end) as i64))
            }
            NumLimit::None => Value::Num(NumValue::Signed(rng.gen::<i16>() as i64)),
        },
        NumInfo::I32(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Signed(*vals.choose(&mut rng).unwrap() as i64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Signed(rng.gen_range(r.start, r.end) as i64))
            }
            NumLimit::None => Value::Num(NumValue::Signed(rng.gen::<i32>() as i64)),
        },
        NumInfo::I64(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Signed(*vals.choose(&mut rng).unwrap() as i64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Signed(rng.gen_range(r.start, r.end) as i64))
            }
            NumLimit::None => Value::Num(NumValue::Signed(rng.gen::<i64>() as i64)),
        },
        NumInfo::U8(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Unsigned(*vals.choose(&mut rng).unwrap() as u64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Unsigned(rng.gen_range(r.start, r.end) as u64))
            }
            NumLimit::None => Value::Num(NumValue::Unsigned(rng.gen::<u8>() as u64)),
        },
        NumInfo::U16(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Unsigned(*vals.choose(&mut rng).unwrap() as u64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Unsigned(rng.gen_range(r.start, r.end) as u64))
            }
            NumLimit::None => Value::Num(NumValue::Unsigned(rng.gen::<u16>() as u64)),
        },
        NumInfo::U32(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Unsigned(*vals.choose(&mut rng).unwrap() as u64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Unsigned(rng.gen_range(r.start, r.end) as u64))
            }
            NumLimit::None => Value::Num(NumValue::Unsigned(rng.gen::<u32>() as u64)),
        },
        NumInfo::U64(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Unsigned(*vals.choose(&mut rng).unwrap() as u64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Unsigned(rng.gen_range(r.start, r.end) as u64))
            }
            NumLimit::None => Value::Num(NumValue::Unsigned(rng.gen::<u64>() as u64)),
        },
        NumInfo::Usize(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Unsigned(*vals.choose(&mut rng).unwrap() as u64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Unsigned(rng.gen_range(r.start, r.end) as u64))
            }
            NumLimit::None => Value::Num(NumValue::Unsigned(rng.gen::<usize>() as u64)),
        },
        NumInfo::Isize(l) => match l {
            NumLimit::Vals(vals) => {
                Value::Num(NumValue::Signed(*vals.choose(&mut rng).unwrap() as i64))
            }
            NumLimit::Range(r) => {
                Value::Num(NumValue::Signed(rng.gen_range(r.start, r.end) as i64))
            }
            NumLimit::None => Value::Num(NumValue::Signed(rng.gen::<isize>() as i64)),
        },
    }
}

fn choose_seq(rs: &RTable, conf: &Config) -> Vec<usize> {
    assert!(!rs.is_empty());

    // selection prability list
    let mut sps = std::iter::repeat(1.0).take(rs.len()).collect::<Vec<_>>();
    let mut seq = Vec::new();
    let mut i;
    while !should_stop(seq.len(), &conf) {
        let index = choose_call(&sps);
        sps[index] *= conf.sp_delta;
        seq.push(index);
        i = seq.len() - 1;
        push_deps(rs, &mut seq, i, &mut sps, conf);
    }

    seq.shrink_to_fit();
    seq.reverse();
    assert!(seq.len() >= conf.prog_min_len);
    seq
}

fn should_stop(prog_len: usize, conf: &Config) -> bool {
    let crt_progress = (prog_len as f64) / (conf.prog_max_len as f64);
    !(prog_len < conf.prog_min_len
        || (prog_len < conf.prog_max_len && random::<f64>() > crt_progress))
}

fn choose_call(sps: &[f64]) -> usize {
    let mut rng = thread_rng();
    let mut cum_sum = std::iter::repeat(0.0).take(sps.len()).collect::<Vec<_>>();
    let mut pre = 0.0;

    for (i, sum) in cum_sum.iter_mut().enumerate() {
        *sum += sps[i] + pre;
        pre = *sum;
    }

    let p = rng.gen_range(0.0, *cum_sum.last().unwrap());
    for (i, sum) in cum_sum.into_iter().enumerate() {
        if p < sum {
            return i;
        }
    }
    unreachable!()
}

#[allow(clippy::collapsible_if)]
fn push_deps(rs: &RTable, seq: &mut Vec<usize>, mut i: usize, sps: &mut [f64], conf: &Config) {
    let mut call_index;

    while !should_stop(seq.len(), &conf) && i < seq.len() {
        call_index = seq[i];
        for (j, r) in rs.index_axis(Axis(0), call_index).iter().enumerate() {
            if call_index != j && random::<f64>() < sps[j] {
                if *r == Relation::Some || random::<f64>() < 0.05 {
                    sps[j] *= conf.sp_delta;
                    seq.push(j);
                }
            }
        }
        i += 1;
    }
}
