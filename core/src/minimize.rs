use crate::prog::{ArgIndex, Call, Prog};
use crate::value::Value;

pub fn minimize<F>(p: &Prog, mut eq: F) -> Prog
where
    F: FnMut(&Prog) -> bool,
{
    let mut p = p.clone();
    let mut p_orig;
    let mut i = 0;
    while i != p.len() - 1 {
        p_orig = p.clone();
        if !remove(&mut p, i) || !eq(&p) {
            i += 1;
            p = p_orig;
        }
    }
    p
}

fn remove(p: &mut Prog, i: usize) -> bool {
    let calls = find_calls(p, i);
    if calls.is_empty() {
        return false;
    }
    // adjust ref arg
    for (j, call) in p.calls.iter_mut().enumerate().skip(i + 1) {
        if !calls.contains(&j) {
            for arg in call.args.iter_mut() {
                for_each_ref_mut(&mut arg.val, |(ref mut cid, _)| {
                    let count =
                        calls
                            .iter()
                            .fold(0, |acc, x| if *cid > *x { acc + 1 } else { acc });
                    *cid -= count;
                });
            }
        }
    }
    let mut removed = 0;
    for c in calls.into_iter() {
        p.calls.remove(c - removed);
        removed += 1;
    }
    true
}

fn find_calls(p: &Prog, i: usize) -> Vec<usize> {
    let last_call = p.len() - 1;
    let mut result = vec![i];
    for (j, call) in p.calls.iter().enumerate().skip(i + 1) {
        if is_arg_ref(call, i) {
            if j == last_call {
                return Default::default();
            }
            let calls = find_calls(p, j);
            if calls.is_empty() {
                return Default::default();
            }
            result.extend(calls);
        }
    }
    result.sort();
    result
}

fn is_arg_ref(call: &Call, i: usize) -> bool {
    let mut result = false;

    for arg in call.args.iter() {
        for_each_ref(&arg.val, |(cid, _): &ArgIndex| {
            if *cid == i {
                result = true;
            }
        });
        if result {
            return true;
        }
    }
    false
}

fn for_each_ref<F: FnMut(&ArgIndex)>(val: &Value, f: F) {
    struct InnerF<F: FnMut(&ArgIndex)> {
        f: Box<F>,
    }
    let mut f = InnerF { f: Box::new(f) };

    fn do_for_each_ref(val: &Value, f: &mut dyn FnMut(&ArgIndex)) {
        use Value::*;
        match val {
            Num(_) | Str(_) | None => {}
            Group(vals) => {
                for v in vals.iter() {
                    do_for_each_ref(v, f)
                }
            }
            Opt { val, .. } => do_for_each_ref(val, f),
            Ref(index) => f(index),
        }
    }

    do_for_each_ref(val, &mut f.f)
}

fn for_each_ref_mut<F: FnMut(&mut ArgIndex)>(val: &mut Value, f: F) {
    struct InnerF<F: FnMut(&mut ArgIndex)> {
        f: Box<F>,
    }
    let mut f = InnerF { f: Box::new(f) };
    fn do_for_each_ref_mut(val: &mut Value, f: &mut dyn FnMut(&mut ArgIndex)) {
        use Value::*;
        match val {
            Num(_) | Str(_) | None => {}
            Group(ref mut vals) => {
                for v in vals.iter_mut() {
                    do_for_each_ref_mut(v, f)
                }
            }
            Opt { ref mut val, .. } => do_for_each_ref_mut(val, f),
            Ref(ref mut index) => f(index),
        }
    }

    do_for_each_ref_mut(val, &mut f.f)
}
